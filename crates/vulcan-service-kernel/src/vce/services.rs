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
            state,
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
    reconcile_vce_project_handles(
        state,
        manager.as_ref(),
        watcher.as_ref(),
        &project,
        &readiness,
    )
    .await
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
        || !manager.project_can_serve_now(project_id, project.vce.revision, expected_fingerprint)
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
/// A provider-independent fail-closed status suitable for the fixed ContextEngine settings page.
/// 返回适用于固定 ContextEngine 设置页且不依赖 Provider 的失败关闭状态。
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
            read_identity: None,
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
            read_identity: None,
            configuration: readiness.status,
            status: disabled_status(&project),
            settings,
            ai: None,
            counts: None,
            integrity: None,
        });
    }
    let manager = manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))?;
    if manager.project_status(&project.id).await.is_none() {
        // Lazy restoration reads existing storage only and never initializes a project.
        // 惰性恢复只读取现有存储，绝不初始化项目。
        let dimension = readiness
            .embedding_dimension
            .ok_or_else(|| String::from("VCE 就绪状态缺少明确向量维度。"))?;
        manager
            .restore_existing_project(
                &project.id,
                &PathBuf::from(&project.path),
                project.vce.clone(),
                dimension,
                readiness
                    .embedding_physical_fingerprint
                    .as_deref()
                    .ok_or_else(|| String::from("VCE 就绪状态缺少向量物理指纹。"))?,
            )
            .await?;
    }
    let Some(status) = manager.project_status(&project.id).await else {
        return Ok(api::VceProjectOverviewResponse {
            read_identity: None,
            configuration: readiness.status,
            status: uninitialized_status(&project),
            settings,
            ai: None,
            counts: None,
            integrity: None,
        });
    };
    let (read_identity, snapshot) = if status.engine.is_some() {
        let (identity, snapshot) = manager.management_snapshot(&project.id).await?;
        (identity, Some(snapshot))
    } else {
        (None, None)
    };
    // Management scans may outlive a settings save or project replacement; reject the retired authority.
    // 管理扫描可能跨越设置保存或项目替换；拒绝已退役的权威配置。
    let current_readiness = vce_configuration_readiness(&state);
    let (current_project, _, _) = vce_project_runtime_inputs(&state, &project.id)?;
    if current_project.path != project.path
        || current_project.vce != project.vce
        || current_readiness.status != readiness.status
        || current_readiness.embedding_physical_fingerprint
            != readiness.embedding_physical_fingerprint
    {
        return Err(String::from("语义引擎配置已变化，请重新读取项目状态。"));
    }
    let mut overview = vce_overview_from_runtime(readiness.status, settings, status, snapshot);
    overview.read_identity = read_identity;
    Ok(overview)
}

/// Read lightweight project activity without management scans or waiting on a running query.
/// 读取轻量项目活动，不进行管理扫描，不等待正在运行的查询。
/// `state` owns configuration and `request` selects one explicit project. Returns committed metadata.
/// `state` 持有配置，`request` 选择一个明确项目。返回已提交元数据。
pub(crate) async fn service_get_vce_project_activity(
    state: SharedServiceBridgeState,
    request: api::VceProjectRequest,
) -> Result<api::VceProjectActivityResponse, String> {
    let readiness = vce_configuration_readiness(&state);
    let (project, manager, _) = vce_project_runtime_inputs(&state, &request.project_id)?;
    let settings = vce_settings_summary(&project.vce);
    let mut status = if !project.vce.enabled {
        disabled_status(&project)
    } else if !readiness.status.ready {
        blocked_configuration_status(&project, &readiness.status)
    } else {
        uninitialized_status(&project)
    };
    let mut query_available = false;
    let mut last_successful_update_unix_ms = None;
    // Missing observation is explicitly unknown; status reads must never create a scan.
    // 缺失观察明确为未知；状态读取绝不创建扫描。
    let mut source_freshness = super::watcher::VceSourceFreshness::default();
    if let Some(manager) = manager {
        if project.vce.enabled && readiness.status.ready {
            let dimension = readiness
                .embedding_dimension
                .ok_or_else(|| String::from("VCE 就绪状态缺少明确向量维度。"))?;
            let fingerprint = readiness
                .embedding_physical_fingerprint
                .as_deref()
                .ok_or_else(|| String::from("VCE 就绪状态缺少向量物理指纹。"))?;
            if manager.project_status(&project.id).await.is_none() {
                manager
                    .restore_existing_project(
                        &project.id,
                        &PathBuf::from(&project.path),
                        project.vce.clone(),
                        dimension,
                        fingerprint,
                    )
                    .await?;
            }
            if let Some(runtime_status) = manager.project_status(&project.id).await {
                status = vce_overview_from_runtime(
                    readiness.status.clone(),
                    settings.clone(),
                    runtime_status,
                    None,
                )
                .status;
            }
            query_available =
                manager.project_can_serve_now(&project.id, project.vce.revision, fingerprint);
        }
        // Read persisted success even when current global models are unavailable; never invoke models here.
        // 即使当前全局模型不可用，也读取持久成功时间；此处绝不调用模型。
        last_successful_update_unix_ms = manager.last_successful_update(&project.id).await?;
        source_freshness = manager.source_freshness(&project.id).await;
    } else if project.vce.enabled && readiness.status.ready {
        return Err(String::from("VCE 运行时尚未就绪。"));
    }
    // Reject a response whose authority changed during asynchronous metadata reads.
    // 拒绝异步元数据读取期间权威配置已经变化的响应。
    let current_readiness = vce_configuration_readiness(&state);
    let (current_project, _, _) = vce_project_runtime_inputs(&state, &project.id)?;
    if current_project.path != project.path
        || current_project.vce != project.vce
        || current_readiness.status != readiness.status
        || current_readiness.embedding_physical_fingerprint
            != readiness.embedding_physical_fingerprint
    {
        return Err(String::from("语义引擎配置已变化，请重新读取项目状态。"));
    }
    Ok(api::VceProjectActivityResponse {
        configuration: readiness.status,
        settings,
        status,
        query_available,
        last_successful_update_unix_ms,
        source_freshness: api::VceSourceFreshnessSummary {
            watching: source_freshness.watching,
            pending_changes: source_freshness.pending_changes,
            reconciliation_required: source_freshness.reconciliation_required,
        },
    })
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
    // Project intent is independently persistable; only indexing and retrieval require global readiness.
    // 项目意图可以独立持久化；仅索引和检索要求全局就绪。
    let project_id = stable_project_id(&request.project_id)?;
    let persist_lock = webui_workspace_persist_lock();
    let persist_guard = persist_lock.lock().await;
    // Hold the existing publication transaction through authoritative snapshot replacement, not runtime drain.
    // 持有现有发布事务直到权威快照替换，不跨运行时排空持有。
    let publication_guard = crate::begin_public_ai_configuration_transaction().await;
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
    // Publish persisted project authority before a fallible full workbench projection can fail.
    // 在可能失败的完整工作台投影前，先发布已持久化项目权威。
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("Lock persisted VCE project authority: {error}"))?;
        let current = guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| {
                workspace
                    .task_projects
                    .iter_mut()
                    .find(|item| item.id == project_id)
            })
            .ok_or_else(|| {
                String::from("VCE settings were saved, but the project left the active workspace")
            })?;
        if current.vce.revision > next.revision {
            return Err(String::from(
                "VCE settings were saved, but a newer project revision is already active",
            ));
        }
        current.vce = next.clone();
    }
    refresh_workbench_snapshot(state.clone())
        .await
        .map_err(|error| format!("VCE 设置已保存，但工作台刷新失败：{error}"))?;
    drop(publication_guard);
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

/// Preserve newer persisted VCE settings during an older asynchronous whole-workbench refresh.
/// 在较旧异步整工作台刷新期间保留较新的持久化 VCE 设置。
/// `loaded` and `retained` are exact project rows, `projected` is their WebUI view; only VCE fields change.
/// `loaded` 与 `retained` 为精确项目记录，`projected` 为其界面投影；仅改变 VCE 字段。
/// Returns an explicit conflict when equal revisions claim different settings, never a guessed winner.
/// 相同修订声称不同设置时返回明确冲突，绝不猜测胜者。
pub(crate) fn preserve_newer_vce_project_settings(
    loaded: &mut [TaskProjectSummary],
    retained: &[TaskProjectSummary],
    projected: &mut [api::ProjectSummary],
) -> Result<(), String> {
    // Indexed exact IDs avoid quadratic work when many projects are present; titles and roots are not keys.
    // 按精确标识建立索引，避免大量项目时的平方复杂度；标题和根路径不作为键。
    let retained_by_id = retained
        .iter()
        .map(|project| (project.id.as_str(), &project.vce))
        .collect::<std::collections::BTreeMap<_, _>>();
    for project in loaded.iter_mut() {
        if let Some(settings) = retained_by_id.get(project.id.as_str()) {
            if settings.revision == project.vce.revision && **settings != project.vce {
                return Err(format!(
                    "VCE project `{}` has conflicting settings at revision {}",
                    project.id, settings.revision
                ));
            }
            if settings.revision > project.vce.revision {
                project.vce = (*settings).clone();
            }
        }
    }
    let loaded_by_id = loaded
        .iter()
        .map(|project| (project.id.as_str(), &project.vce))
        .collect::<std::collections::BTreeMap<_, _>>();
    for project in projected {
        if let Some(settings) = loaded_by_id.get(project.id.as_str()) {
            project.vce_enabled = settings.enabled;
            project.vce_settings_revision = settings.revision;
        }
    }
    Ok(())
}

/// Admit an explicit human indexing job and return a durable receipt without waiting for indexing.
/// 准入明确人为索引作业并返回持久回执，不等待索引完成。
///
/// `state` owns project/model authority and the retained executor; `request` contains optimistic guards.
/// Returns the acknowledged job; no startup or setting-change path calls this mutation.
/// `state` 持有项目、模型权威与保留执行器；`request` 包含乐观保护。
/// 返回已确认作业；启动或设置变化路径均不调用此修改入口。
pub(crate) async fn service_submit_vce_job(
    state: SharedServiceBridgeState,
    request: api::VceJobSubmitRequest,
) -> Result<api::VceJobSummary, String> {
    let readiness = require_vce_configuration_ready(&state)?;
    let (project, manager, _) = vce_project_runtime_inputs(&state, &request.project_id)?;
    validate_project_revision(&project, request.expected_settings_revision)?;
    if !project.vce.enabled {
        return Err(String::from("请先在项目设置中启用语义引擎。"));
    }
    if request.idempotency_key.trim().is_empty() || request.idempotency_key.len() > 512 {
        return Err(String::from("索引请求缺少有效的幂等标识。"));
    }
    let manager = manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))?;
    let executor = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .service_runtime
        .clone()
        .ok_or_else(|| String::from("VCE 后台执行器尚未就绪。"))?;
    let dimension = readiness
        .embedding_dimension
        .ok_or_else(|| String::from("VCE 就绪状态缺少明确向量维度。"))?;
    let fingerprint = readiness
        .embedding_physical_fingerprint
        .ok_or_else(|| String::from("VCE 就绪状态缺少向量物理指纹。"))?;
    let initialize = request.action == api::VceJobAction::Initialize;
    manager
        .enable_project(
            &project.id,
            &PathBuf::from(&project.path),
            project.vce.clone(),
            dimension,
            &fingerprint,
            initialize,
        )
        .await?;
    // Validate again after storage admission; stale public model/project intent cannot launch work.
    // 存储准入后再次校验；过期公共模型或项目意图不能启动工作。
    let current = require_vce_configuration_ready(&state)?;
    let current_project = vce_project_runtime_inputs(&state, &project.id)?.0;
    validate_project_revision(&current_project, request.expected_settings_revision)?;
    if !current_project.vce.enabled
        || current.embedding_physical_fingerprint.as_deref() != Some(&fingerprint)
    {
        return Err(String::from(
            "项目或模型配置已变化，请刷新后重新提交索引作业。",
        ));
    }
    let receipt = manager
        .submit_job(
            &project.id,
            super::VceJobAdmission {
                retry_of_job_id: request.retry_of_job_id,
                kind: if initialize {
                    vulcan_workspace_store::vce_data_plane::VceJobKind::Initialize
                } else {
                    vulcan_workspace_store::vce_data_plane::VceJobKind::Synchronize
                },
                idempotency_key: request.idempotency_key,
                expected_settings_revision: request.expected_settings_revision,
                expected_checkpoint_version: request.expected_checkpoint_version,
                contract_fingerprint: fingerprint,
            },
            &executor,
        )
        .await?;
    Ok(map_vce_job(receipt))
}

/// Read durable job status even if a project or model was subsequently disabled.
/// 即使项目或模型随后关闭，也读取持久作业状态。
///
/// `state` validates project ownership; `request` selects an exact job or the latest receipt.
/// Returns no receipt when this project has never admitted a job; no models or source are invoked.
/// `state` 校验项目归属；`request` 选择精确作业或最近回执。
/// 项目从未准入作业时返回空值；不调用模型或源码。
pub(crate) async fn service_get_vce_job(
    state: SharedServiceBridgeState,
    request: api::VceJobQueryRequest,
) -> Result<Option<api::VceJobSummary>, String> {
    let (project, manager, _) = vce_project_runtime_inputs(&state, &request.project_id)?;
    let manager = manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))?;
    manager
        .get_job(&project.id, request.job_id.as_deref())
        .await
        .map(|receipt| receipt.map(map_vce_job))
}

/// Cancel only the stable displayed job without requiring the current models to remain available.
/// 仅取消稳定展示作业，不要求当前模型仍然可用。
///
/// `state` validates project ownership and `request` supplies exact project/job identities.
/// Returns the durable cancellation receipt or the already settled result.
/// `state` 校验项目归属，`request` 提供精确项目与作业身份。返回持久取消回执或已确定结果。
pub(crate) async fn service_cancel_vce_job(
    state: SharedServiceBridgeState,
    request: api::VceJobCancelRequest,
) -> Result<api::VceJobSummary, String> {
    let (project, manager, _) = vce_project_runtime_inputs(&state, &request.project_id)?;
    let manager = manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))?;
    manager
        .cancel_job(&project.id, &request.job_id)
        .await
        .map(map_vce_job)
}

/// Project storage-owned receipts into an explicit browser contract without internal ownership keys.
/// 将存储持有回执投影为明确浏览器契约，不含内部归属键。
///
/// `job` is already validated by storage. Returns safe immutable display metadata.
/// `job` 已由存储校验。返回安全不可变显示元数据。
pub(crate) fn map_vce_job(
    job: vulcan_workspace_store::vce_data_plane::VceStoredJob,
) -> api::VceJobSummary {
    use vulcan_context_engine::{VceRefreshPhase, VceRefreshUnit};
    use vulcan_workspace_store::vce_data_plane::{VceJobKind, VceJobState};
    api::VceJobSummary {
        project_id: job.project_id,
        job_id: job.job_id,
        kind: match job.kind {
            VceJobKind::Initialize => api::VceJobKindSummary::Initialize,
            VceJobKind::Synchronize => api::VceJobKindSummary::Synchronize,
            VceJobKind::Rebuild => api::VceJobKindSummary::Rebuild,
            VceJobKind::Compact => api::VceJobKindSummary::Compact,
        },
        state: match job.state {
            VceJobState::Queued => api::VceJobStateSummary::Queued,
            VceJobState::Running => api::VceJobStateSummary::Running,
            VceJobState::Cancelling => api::VceJobStateSummary::Cancelling,
            VceJobState::Succeeded => api::VceJobStateSummary::Succeeded,
            VceJobState::Failed => api::VceJobStateSummary::Failed,
            VceJobState::Cancelled => api::VceJobStateSummary::Cancelled,
            VceJobState::Interrupted => api::VceJobStateSummary::Interrupted,
        },
        sequence: job.sequence,
        created_at_unix_ms: job.created_at_unix_ms,
        updated_at_unix_ms: job.updated_at_unix_ms,
        source_checkpoint_version: job.source_checkpoint_version,
        retry_of_job_id: job.retry_of_job_id,
        result_checkpoint_version: job.result_checkpoint_version,
        progress: job.progress.map(|progress| api::VceJobProgressSummary {
            phase: match progress.phase {
                VceRefreshPhase::Scanning => "scanning",
                VceRefreshPhase::Reconciling => "reconciling",
                VceRefreshPhase::Chunking => "chunking",
                VceRefreshPhase::Embedding => "embedding",
                VceRefreshPhase::Writing => "writing",
                VceRefreshPhase::Committing => "committing",
                VceRefreshPhase::Complete => "complete",
            }
            .to_owned(),
            operation_id: progress.operation_id,
            unit: match progress.unit {
                VceRefreshUnit::Files => "files",
                VceRefreshUnit::Blobs => "blobs",
                VceRefreshUnit::Batches => "batches",
                VceRefreshUnit::Tables => "tables",
                VceRefreshUnit::None => "none",
            }
            .to_owned(),
            completed: progress.completed,
            total: progress.total,
            overall_completed: progress.overall_completed,
            overall_total: progress.overall_total,
            current_path: progress.current_path,
        }),
        compaction: job.compaction.map(map_compaction_job),
        followup_requested: job.followup_requested,
        error: job.error,
    }
}

/// Project one durable physical compaction payload into management-safe public state.
/// 将一个持久物理整理载荷投影为管理安全的公共状态。
fn map_compaction_job(
    compaction: vulcan_workspace_store::vce_data_plane::VceCompactionJobReceipt,
) -> api::VceCompactionJobSummary {
    use vulcan_workspace_store::vce_data_plane::VceCompactionTableState;
    api::VceCompactionJobSummary {
        tables: compaction
            .tables
            .into_iter()
            .map(|table| {
                let state = match table.state {
                    VceCompactionTableState::Pending => {
                        api::VceCompactionTableStateSummary::Pending
                    }
                    VceCompactionTableState::Queued => api::VceCompactionTableStateSummary::Queued,
                    VceCompactionTableState::Running => {
                        api::VceCompactionTableStateSummary::Running
                    }
                    VceCompactionTableState::Succeeded => {
                        api::VceCompactionTableStateSummary::Succeeded
                    }
                    VceCompactionTableState::Failed => api::VceCompactionTableStateSummary::Failed,
                    VceCompactionTableState::Cancelled => {
                        api::VceCompactionTableStateSummary::Cancelled
                    }
                    VceCompactionTableState::StateLost => {
                        api::VceCompactionTableStateSummary::StateLost
                    }
                    VceCompactionTableState::Interrupted => {
                        api::VceCompactionTableStateSummary::Interrupted
                    }
                };
                api::VceCompactionTableJobSummary {
                    role: map_physical_table_role(table.plan.statistics.role),
                    state,
                    round: table.round,
                    operation_id: table.operation_id,
                    planned_version: table.plan.statistics.version,
                    current_version: table.latest_statistics.version,
                    planned_fragment_count: table.plan.statistics.fragment_count,
                    current_fragment_count: table.latest_statistics.fragment_count,
                    metrics: api::VceCompactionMetricsSummary {
                        fragments_removed: table.metrics.fragments_removed,
                        fragments_added: table.metrics.fragments_added,
                        files_removed: table.metrics.files_removed,
                        files_added: table.metrics.files_added,
                    },
                    error: table.error,
                }
            })
            .collect(),
    }
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
    match execute_management_query(
        &manager,
        &request.project_id,
        query,
        Some(&request.expected_read_identity),
    )
    .await?
    {
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
    match execute_management_query(
        &manager,
        &request.project_id,
        query,
        Some(&request.expected_read_identity),
    )
    .await?
    {
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
    match execute_management_query(&manager, &request.project_id, query, None).await? {
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
    match execute_management_query(
        &manager,
        &request.project_id,
        query,
        request.expected_read_identity.as_deref(),
    )
    .await?
    {
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
    let mode = parse_maintenance_mode(&request.mode)?;
    // Physical compaction reads only the stored index contract; model-producing modes retain the
    // live model-readiness gate before any maintenance target is exposed.
    // 物理整理只读取已存索引契约；会产生模型结果的模式仍在暴露维护目标前保留实时模型就绪门禁。
    let manager = if mode == VceMaintenanceMode::Compact {
        vce_manager_for_stored_project(&state, &request.project_id)?
    } else {
        vce_manager_for_project(&state, &request.project_id)?
    };
    let target = manager
        .stored_maintenance_target(&request.project_id, mode)
        .await?;
    if mode == VceMaintenanceMode::Compact {
        return Ok(map_maintenance_plan(
            target.plan,
            target.embedding_fingerprint,
            target.embedding_dimension,
            target.checkpoint_version,
        ));
    }
    // Model-producing maintenance retains the live readiness identity while its plan and checkpoint
    // come from the same stored snapshot. Compact mode returned above and never requires readiness.
    // 会生产模型结果的维护保留实时就绪身份，而计划与检查点来自同一已存快照。
    // compact 模式已在上方返回，绝不要求就绪状态。
    let readiness = require_vce_configuration_ready(&state)?;
    Ok(map_maintenance_plan(
        target.plan,
        readiness
            .embedding_physical_fingerprint
            .ok_or_else(|| String::from("VCE 就绪状态缺少向量物理指纹。"))?,
        readiness
            .embedding_dimension
            .ok_or_else(|| String::from("VCE 就绪状态缺少明确向量维度。"))?,
        target.checkpoint_version,
    ))
}

/// Execute an exact current maintenance plan under settings and checkpoint guards.
/// 在设置与检查点保护下执行一份精确的当前维护计划。
pub(crate) async fn service_execute_vce_maintenance(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: api::VceMaintenanceExecutionRequest,
) -> Result<api::VceMaintenanceExecutionResponse, String> {
    let (project, manager, _) = vce_project_runtime_inputs(&state, &request.project_id)?;
    validate_project_revision(&project, request.expected_settings_revision)?;
    if !project.vce.enabled {
        return Err(String::from("请先在项目设置中启用语义引擎。"));
    }
    let manager = manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))?;
    let mode = parse_maintenance_mode(&request.mode)?;
    validate_maintenance_checkpoint_requirement(mode, request.expected_checkpoint_version)?;
    // Exact action sets enter durable admission before current-plan reads, so completed retries can replay.
    // 精确动作集合在读取当前计划前进入持久准入，使已完成请求能够重放。
    let requested_action_count = request.action_ids.len();
    let requested_ids = request.action_ids.into_iter().collect::<BTreeSet<_>>();
    if requested_ids.len() != requested_action_count {
        return Err(String::from("VCE 维护操作包含重复身份，请重新生成计划。"));
    }
    if mode == VceMaintenanceMode::Compact {
        // Physical compaction binds only to the stored index contract and never activates a model.
        // 物理整理仅绑定已存索引契约，绝不激活模型。
        let executor = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .service_runtime
            .clone()
            .ok_or_else(|| String::from("VCE 后台执行器尚未就绪。"))?;
        let receipt = manager
            .submit_physical_compaction(
                &project.id,
                super::VceJobAdmission {
                    retry_of_job_id: None,
                    kind: vulcan_workspace_store::vce_data_plane::VceJobKind::Compact,
                    idempotency_key: request.idempotency_key,
                    expected_settings_revision: request.expected_settings_revision,
                    expected_checkpoint_version: request.expected_checkpoint_version,
                    contract_fingerprint: request.expected_embedding_fingerprint,
                },
                requested_ids,
                &executor,
            )
            .await?;
        return Ok(api::VceMaintenanceExecutionResponse::Job {
            job: Box::new(map_vce_job(receipt)),
        });
    }
    // Model-dependent rebuild and legacy cleanup retain their existing live readiness guard.
    // 依赖模型的重建与既有清理保留原有实时就绪保护。
    let readiness = require_vce_configuration_ready(&state)?;
    if matches!(
        mode,
        VceMaintenanceMode::Repair | VceMaintenanceMode::Rebuild
    ) {
        // executor retains the admitted task after this request or its page has returned.
        // executor 在本请求或页面返回后继续持有已准入任务。
        let executor = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .service_runtime
            .clone()
            .ok_or_else(|| String::from("VCE 后台执行器尚未就绪。"))?;
        // fingerprint preserves the confirmed preview so exact completed submissions can replay before new admission.
        // fingerprint 保留已确认预览，使精确已完成提交可在新准入前重放。
        let fingerprint = request.expected_embedding_fingerprint;
        // receipt acknowledges admission only; observers report actual indexing completion.
        // receipt 仅确认准入；观察器报告实际索引完成。
        let receipt = manager
            .submit_project_rebuild(
                super::VceRebuildProjectInput {
                    project_id: project.id.clone(),
                    project_root: std::path::PathBuf::from(&project.path),
                    settings: project.vce.clone(),
                    dimension: readiness
                        .embedding_dimension
                        .ok_or_else(|| String::from("VCE 缺少明确向量维度。"))?,
                    fingerprint: readiness
                        .embedding_physical_fingerprint
                        .clone()
                        .ok_or_else(|| String::from("VCE 缺少当前向量契约。"))?,
                },
                super::VceJobAdmission {
                    retry_of_job_id: None,
                    kind: vulcan_workspace_store::vce_data_plane::VceJobKind::Rebuild,
                    idempotency_key: request.idempotency_key,
                    expected_settings_revision: request.expected_settings_revision,
                    expected_checkpoint_version: request.expected_checkpoint_version,
                    contract_fingerprint: fingerprint,
                },
                mode == VceMaintenanceMode::Repair,
                requested_ids,
                &executor,
            )
            .await?;
        return Ok(api::VceMaintenanceExecutionResponse::Job {
            job: Box::new(map_vce_job(receipt)),
        });
    }
    // Cleanup has no indexing receipt; retain its explicit current-checkpoint and plan checks.
    // 清理没有索引回执；保留其明确的当前检查点及计划校验。
    if readiness.embedding_physical_fingerprint.as_deref()
        != Some(request.expected_embedding_fingerprint.as_str())
    {
        return Err(String::from(
            "VCE 向量配置已在维护预览后变化，请重新生成计划并确认。",
        ));
    }
    let expected_checkpoint_version = request
        .expected_checkpoint_version
        .ok_or_else(|| String::from("修复或暂存清理要求一个已发布检查点。"))?;
    validate_checkpoint(&manager, &project.id, Some(expected_checkpoint_version)).await?;
    let plan = load_maintenance_plan(&manager, &project.id, mode).await?;
    let planned_ids = plan
        .actions
        .iter()
        .map(|action| action.action_id.clone())
        .collect::<BTreeSet<_>>();
    if planned_ids.is_empty() || requested_ids != planned_ids {
        return Err(String::from("VCE 维护操作已经变化，请重新生成计划并确认。"));
    }
    manager.cleanup_staging(&project.id).await?;
    emit_vce_project_changed(&app, &state, &project.id).await;
    // overview describes cleanup only, which never scans files or invokes a model.
    // overview 只描述清理；清理绝不扫描文件或调用模型。
    let overview = service_get_vce_project_overview(
        state,
        api::VceProjectRequest {
            project_id: project.id,
        },
    )
    .await?;
    Ok(api::VceMaintenanceExecutionResponse::Cleaned {
        overview: Box::new(overview),
    })
}

/// Reconcile one project against its current settings and watcher handles.
/// 根据当前设置与监听句柄协调单个项目。
/// `state` provides live authority; `manager` and `watcher` are optional startup handles.
/// `state` 提供实时权威；`manager` 与 `watcher` 是可选启动句柄。
/// `project` and `readiness` are the captured decision inputs; returns success or an explicit stale/lifecycle error.
/// `project` 与 `readiness` 是已捕获决策输入；返回成功或明确的过期／生命周期错误。
async fn reconcile_vce_project_handles(
    state: &SharedServiceBridgeState,
    manager: Option<&VceRuntimeManager>,
    watcher: Option<&VceWatcherService>,
    project: &TaskProjectSummary,
    readiness: &VceConfigurationReadiness,
) -> Result<(), String> {
    // Validate inside the manager's existing lifecycle gate, after every lock acquisition that can suspend.
    // 在管理器既有生命周期闸门内校验，位于所有可能挂起的取锁操作之后。
    let validate_current =
        || validate_vce_reconciliation_snapshot(state, manager, project, readiness);
    if let Some(manager) = manager {
        return manager
            .reconcile_project_configuration(project, readiness, watcher, validate_current)
            .await;
    }
    // Bootstrap only creates a watcher with a manager; an absent manager cannot own an asynchronous mutation.
    // 启动只在存在管理器时创建监听器；缺失管理器不能持有异步变更。
    validate_current()?;
    if watcher.is_some() || (project.vce.enabled && readiness.status.ready) {
        return Err(String::from("VCE 运行时不可用。"));
    }
    Ok(())
}

/// Validate `project`, `readiness`, and optional `manager` against the live host `state` without mutating it.
/// 对照实时宿主 `state` 校验 `project`、`readiness` 及可选 `manager`，不修改宿主。
/// The caller holds the project lifecycle gate whenever a manager exists; errors reject retired callbacks.
/// 存在管理器时调用方持有项目生命周期闸门；错误用于拒绝已退役回调。
fn validate_vce_reconciliation_snapshot(
    state: &SharedServiceBridgeState,
    manager: Option<&VceRuntimeManager>,
    project: &TaskProjectSummary,
    readiness: &VceConfigurationReadiness,
) -> Result<(), String> {
    // Re-read the sole project authority and installed runtime generation, not a fallback configuration source.
    // 重读唯一项目权威和已安装运行时代际，不读取备用配置来源。
    let (current_project, current_manager, _) = vce_project_runtime_inputs(state, &project.id)?;
    if current_project.path != project.path || current_project.vce != project.vce {
        return Err(String::from("VCE project reconciliation snapshot is stale"));
    }
    // Optional managers represent startup availability; two present handles must share the exact lifecycle.
    // 可选管理器表示启动可用性；两个存在的句柄必须共享精确生命周期。
    let same_manager = match (manager, current_manager.as_ref()) {
        (Some(expected), Some(current)) => expected.shares_lifecycle_with(current),
        (None, None) => true,
        _ => false,
    };
    if !same_manager {
        return Err(String::from(
            "VCE reconciliation runtime generation is stale",
        ));
    }
    // Compare all decision-bearing readiness facts, including an old unavailable snapshot at the same project revision.
    // 比较所有参与决策的就绪事实，包括同一项目修订下的旧不可用快照。
    let current_readiness = vce_configuration_readiness(state);
    if current_readiness.status.ready != readiness.status.ready
        || current_readiness.embedding_physical_fingerprint
            != readiness.embedding_physical_fingerprint
        || current_readiness.embedding_dimension != readiness.embedding_dimension
    {
        return Err(String::from(
            "VCE configuration reconciliation snapshot is stale",
        ));
    }
    Ok(())
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
                VceProjectLifecycle::Ready
                    | VceProjectLifecycle::Refreshing
                    | VceProjectLifecycle::Failed
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
    vce_manager_for_stored_project(state, project_id)
}

/// Return the VCE runtime for storage-only operations without requiring a live model route.
/// 返回供纯存储操作使用的 VCE 运行时，不要求实时模型路由。
fn vce_manager_for_stored_project(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<VceRuntimeManager, String> {
    let (project, manager, _) = vce_project_runtime_inputs(state, project_id)?;
    if !project.vce.enabled {
        return Err(format!("VCE 项目 `{}` 已禁用。", project.id));
    }
    manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))
}

/// Execute one typed management request with a unique operation identity.
/// 使用唯一操作身份执行一项类型化管理请求。
/// `manager` and `project_id` select the runtime; `query` and its optional read identity bind the result.
/// `manager` 与 `project_id` 选择运行时；`query` 及其可选读取身份绑定结果。
/// Returns a validated result or an explicit stale-publication error without retrying another version.
/// 返回已验证结果或明确过期发布错误，不重试其他版本。
async fn execute_management_query(
    manager: &VceRuntimeManager,
    project_id: &str,
    query: VceManagementQuery,
    expected_read_identity: Option<&str>,
) -> Result<VceManagementQueryResult, String> {
    let response = manager
        .management_query(
            project_id,
            VceManagementQueryRequest {
                operation_id: format!("vce:{project_id}:management:{}", Uuid::new_v4()),
                query,
            },
            expected_read_identity,
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
    manager.stored_maintenance_plan(project_id, mode).await
}

/// Project one core maintenance plan into its public service representation.
/// 将一份内核维护计划投影为公共服务表示。
/// `plan` owns stored actions; fingerprint/dimension freeze the preview target. Returns the confirmation payload.
/// `plan` 持有存储动作；指纹／维度固定预览目标。返回确认载荷。
fn map_maintenance_plan(
    plan: VceMaintenancePlan,
    embedding_fingerprint: String,
    embedding_dimension: u32,
    checkpoint_version: Option<u64>,
) -> api::VceMaintenancePlanResponse {
    api::VceMaintenancePlanResponse {
        embedding_fingerprint,
        embedding_dimension,
        checkpoint_version,
        mode: maintenance_mode_text(plan.mode).to_owned(),
        actions: plan
            .actions
            .into_iter()
            .map(|action| api::VceMaintenanceActionSummary {
                action_id: action.action_id,
                description: action.description,
                blob_names: action.blob_names,
                destructive: action.destructive,
                physical_table_role: action
                    .physical_table
                    .map(|table| map_physical_table_role(table.statistics.role)),
            })
            .collect(),
        affected_blob_count: plan.affected_blob_count,
        physical_tables: plan
            .physical_tables
            .into_iter()
            .map(map_physical_table_plan)
            .collect(),
    }
}

/// Project one core physical table plan into the public preview contract.
/// 将一份内核物理表计划投影到公共预览契约。
fn map_physical_table_plan(
    table: vulcan_context_engine::VcePhysicalTablePlan,
) -> api::VcePhysicalTablePlanSummary {
    let statistics = table.statistics;
    let options = table.options;
    api::VcePhysicalTablePlanSummary {
        role: map_physical_table_role(statistics.role),
        version: statistics.version,
        total_bytes: statistics.total_bytes,
        row_count: statistics.row_count,
        index_count: statistics.index_count,
        fragment_count: statistics.fragment_count,
        small_fragment_count: statistics.small_fragment_count,
        fragment_rows: api::VcePhysicalFragmentSummary {
            minimum_rows: statistics.fragment_rows.minimum_rows,
            maximum_rows: statistics.fragment_rows.maximum_rows,
            mean_rows: statistics.fragment_rows.mean_rows,
            p25_rows: statistics.fragment_rows.p25_rows,
            p50_rows: statistics.fragment_rows.p50_rows,
            p75_rows: statistics.fragment_rows.p75_rows,
            p99_rows: statistics.fragment_rows.p99_rows,
        },
        compaction_required: table.compaction_required,
        options: api::VceCompactionOptionsSummary {
            target_rows_per_fragment: options.target_rows_per_fragment,
            max_rows_per_group: options.max_rows_per_group,
            max_bytes_per_file: options.max_bytes_per_file,
            num_threads: options.num_threads,
            batch_size: options.batch_size,
            max_source_fragments: options.max_source_fragments,
        },
    }
}

/// Map one internal physical table role into its public stable enum.
/// 将一个内部物理表角色映射为公共稳定枚举。
fn map_physical_table_role(
    role: vulcan_context_engine::VcePhysicalTableRole,
) -> api::VcePhysicalTableRoleSummary {
    match role {
        vulcan_context_engine::VcePhysicalTableRole::Content => {
            api::VcePhysicalTableRoleSummary::Content
        }
        vulcan_context_engine::VcePhysicalTableRole::Path => api::VcePhysicalTableRoleSummary::Path,
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
                start_char_offset: hit.start_char_offset,
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
    // Index counts and engine metadata must come from the same pinned management read when present.
    // 存在管理快照时，索引数量与引擎元数据必须来自同一次固定读取。
    let (engine, workspace) = match snapshot.as_ref() {
        Some(snapshot) => (Some(&snapshot.engine), Some(&snapshot.workspace)),
        None => (status.engine.as_ref(), status.workspace.as_ref()),
    };
    // Checkpoint membership comes from the committed core projection, not total stored blobs.
    // 检查点成员来自已提交核心投影，而非全部存储块对象数量。
    let checkpoint_blobs = workspace.map_or(0, |workspace| workspace.checkpoint_blobs);
    api::VceProjectOverviewResponse {
        read_identity: None,
        configuration,
        settings,
        status: api::VceProjectStatusSummary {
            project_id: status.project_id,
            enabled: true,
            settings_revision: status.config_revision,
            lifecycle,
            engine_state: engine.map(|engine| engine_state(engine.state)),
            workspace_generation: engine.map(|engine| engine.workspace_generation),
            checkpoint_version: engine.and_then(|engine| engine.checkpoint_version),
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

/// Project the confirmed absence of an initialized index without inventing a runtime failure.
/// 投影已确认不存在初始化索引的状态，不虚构运行时失败。
///
/// `project` owns the saved enablement and revision. Returns a metadata-only status.
/// `project` 持有保存的启用状态和修订号。返回仅含元数据的状态。
fn uninitialized_status(project: &TaskProjectSummary) -> api::VceProjectStatusSummary {
    api::VceProjectStatusSummary {
        project_id: project.id.clone(),
        enabled: project.vce.enabled,
        settings_revision: project.vce.revision,
        lifecycle: api::VceProjectLifecycleState::Uninitialized,
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
        update_mode: match settings.update_mode {
            vulcan_workbench_model::ProjectVceUpdateMode::Manual => api::VceIndexUpdateMode::Manual,
            vulcan_workbench_model::ProjectVceUpdateMode::Automatic => {
                api::VceIndexUpdateMode::Automatic
            }
        },
        auto_update_min_interval_seconds: settings.auto_update_min_interval_seconds,
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
        update_mode: match input.update_mode {
            api::VceIndexUpdateMode::Manual => vulcan_workbench_model::ProjectVceUpdateMode::Manual,
            api::VceIndexUpdateMode::Automatic => {
                vulcan_workbench_model::ProjectVceUpdateMode::Automatic
            }
        },
        auto_update_min_interval_seconds: input.auto_update_min_interval_seconds,
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
        "compact" => Ok(VceMaintenanceMode::Compact),
        _ => Err(format!("不支持的 VCE 维护模式：`{value}`。")),
    }
}

/// Require a published checkpoint for maintenance modes that do not create a replacement index.
/// 对于不创建替代索引的维护模式，要求调用方提供已发布检查点。
/// `mode` selects whether this execution creates a replacement index; `expected_checkpoint` is the confirmed published version.
/// `mode` 选择本次执行是否创建替代索引；`expected_checkpoint` 是确认时记录的已发布版本。
/// Returns an error before any executor admission or storage mutation when a non-rebuild mode lacks its checkpoint.
/// 当非重建模式缺少检查点时，在任何执行器准入或存储变更前返回错误。
fn validate_maintenance_checkpoint_requirement(
    mode: VceMaintenanceMode,
    expected_checkpoint: Option<u64>,
) -> Result<(), String> {
    if expected_checkpoint.is_none() && mode != VceMaintenanceMode::Rebuild {
        return Err(String::from(
            "修复、暂存清理或物理整理要求一个已发布检查点。",
        ));
    }
    Ok(())
}

/// Return the stable public lifecycle state.
/// 返回稳定公共生命周期状态。
fn lifecycle_state(value: VceProjectLifecycle) -> api::VceProjectLifecycleState {
    match value {
        VceProjectLifecycle::Uninitialized => api::VceProjectLifecycleState::Uninitialized,
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
        VceOperationKind::StorageMutation => "storage_mutation",
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
        VceMaintenanceMode::Compact => "compact",
    }
}

/// Replay a captured obsolete reconciliation against a currently initialized private fixture.
/// 对当前已初始化私有夹具重放已捕获的过期协调。
/// `state` supplies live authority; `retired_project` and `retired_readiness` are earlier snapshots.
/// `state` 提供实时权威；`retired_project` 与 `retired_readiness` 为较早快照。
/// Returns boxed assertions requiring unchanged admission and explicit rejection of obsolete work.
/// 返回装箱断言，要求准入不变并明确拒绝过期工作。
#[cfg(test)]
pub(crate) fn verify_retired_reconciliation_preserves_runtime(
    state: &SharedServiceBridgeState,
    retired_project: TaskProjectSummary,
    retired_readiness: VceConfigurationReadiness,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async move {
        // Capture current ownership before invoking the same production function used by scheduled callbacks.
        // 调用计划回调使用的同一生产函数前捕获当前归属。
        let (project, manager, watcher) = vce_project_runtime_inputs(state, &retired_project.id)
            .expect("current fixture project");
        // This manager is the fixture's actual host, not a mock of lifecycle removal.
        // 此管理器为夹具实际宿主，不是生命周期移除模拟。
        let manager = manager.expect("active fixture manager");
        // Keep exact checkpoint and policy facts to detect removal or rollback by the obsolete callback.
        // 保留精确检查点与策略事实，以检测过期回调引起的移除或回滚。
        let before = manager
            .project_status(&project.id)
            .await
            .expect("current initialized runtime");
        // The current model contract must remain usable across either retired configuration branch.
        // 当前模型契约必须跨两种过期配置分支保持可用。
        let readiness = vce_configuration_readiness(state);
        manager
            .verify_reconciliation_validation_waits_for_lifecycle(&project, &readiness)
            .await;
        // Invoke the actual obsolete decision before asserting, so the red test proves its destructive effect.
        // 在断言前执行实际过期决策，使红色测试证明其破坏性影响。
        let result = reconcile_vce_project_handles(
            state,
            Some(&manager),
            watcher.as_ref(),
            &retired_project,
            &retired_readiness,
        )
        .await;
        // Current runtime identity may not disappear just because an old callback arrives later.
        // 当前运行时身份不能仅因为旧回调晚到就消失。
        let after = manager
            .project_status(&project.id)
            .await
            .expect("retired reconciliation must not remove the current runtime");
        assert_eq!(after.config_revision, before.config_revision);
        assert_eq!(after.lifecycle, before.lifecycle);
        assert_eq!(
            after.engine.and_then(|engine| engine.checkpoint_version),
            before.engine.and_then(|engine| engine.checkpoint_version)
        );
        assert!(
            manager.project_can_serve_now(
                &project.id,
                project.vce.revision,
                readiness
                    .embedding_physical_fingerprint
                    .as_deref()
                    .expect("current fixture fingerprint")
            )
        );
        assert!(
            result.is_err(),
            "obsolete reconciliation must be rejected explicitly"
        );
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preserve exact Unicode character coordinates when projecting core hits to the model-facing wire.
    /// 将核心命中投影为模型侧线协议时保留精确 Unicode 字符坐标。
    /// No inputs or external calls; asserts zero and nonzero offsets with immutable publication identity.
    /// 无输入或外部调用；断言零与非零偏移以及不可变发布身份。
    #[test]
    fn retrieval_projection_preserves_character_offsets() {
        // Both fragments belong to one source line, so line numbers alone cannot distinguish their positions.
        // 两个片段属于同一源码行，因此仅凭行号无法区分位置。
        let blob = vulcan_context_engine::blob_name(
            "src/语义.rs",
            "\n\n\n\n\n\n函数检索与字符定位及重复函数",
        )
        .expect("source identity");
        // Offsets are Unicode scalar counts supplied by the core, never inferred from UTF-8 byte lengths.
        // 偏移为核心提供的 Unicode 标量数，绝不从 UTF-8 字节数推算。
        let hits: Vec<_> = [0, 12]
            .into_iter()
            .map(|offset| {
                // The existing chunk constructor owns occurrence identity and source coordinate validation.
                // 现有代码块构造器负责出现位置身份及源码坐标校验。
                let chunk = vulcan_context_engine::Chunk::new_with_start_char_offset(
                    &blob,
                    offset,
                    "src/语义.rs",
                    "函数",
                    7,
                    7,
                    ChunkKind::Recursive,
                )
                .expect("valid long-line fragment");
                vulcan_context_engine::SearchHit::from_chunk(
                    &chunk,
                    0.9,
                    SearchSource::Dense,
                    HitKind::Content,
                )
            })
            .collect();
        // Planning and formatting use the actual core implementations without registering model services.
        // 规划与格式化使用实际核心实现，不注册模型服务。
        let plan = vulcan_context_engine::QueryPlanner::new(Default::default())
            .expect("planner config")
            .plan("semantic source fragments")
            .expect("query plan");
        // The summary belongs to the same selected hits and request as the projected response.
        // 摘要与投影响应属于相同选中命中及请求。
        let summary = vulcan_context_engine::VceRetrievalSummary {
            request_id: String::from("coordinate-query"),
            intent: plan.intent,
            query_count: plan.queries.len(),
            dense_hit_count: hits.len(),
            path_hit_count: 0,
            exact_hit_count: 0,
            candidate_count: hits.len(),
            selected_hit_count: hits.len(),
            degraded_features: Vec::new(),
            degradations: Vec::new(),
        };
        // Capture the authoritative rendering before the response consumes its selected source hits.
        // 在响应消耗选中源码命中前保存权威渲染结果。
        let formatted_context = vulcan_context_engine::render_context(&hits);
        // This is the production projection used by the native retrieval callback, not a separate wire fixture.
        // 此处使用原生检索回调的生产投影，而非单独构造线协议夹具。
        let response = vce_retrieval_response(
            String::from("coordinate-project"),
            VceRuntimeRetrieveResult {
                settings_revision: 3,
                checkpoint_version: 7,
                result: vulcan_context_engine::RetrieveResult {
                    request_id: summary.request_id.clone(),
                    query_plan: plan,
                    scope: BTreeSet::from([blob]),
                    hits,
                    formatted_context: formatted_context.clone(),
                    degraded_features: Vec::new(),
                    degradations: Vec::new(),
                    summary,
                },
            },
        );
        // Serialize through the same contract as the model callback to detect omitted public fields.
        // 通过与模型回调相同的契约序列化，以检测遗漏的公开字段。
        let wire = to_value(response).expect("serialize retrieval response");
        assert_eq!(wire["projectId"], "coordinate-project");
        assert_eq!(wire["settingsRevision"], 3);
        assert_eq!(wire["checkpointVersion"], 7);
        assert_eq!(wire["hits"][0]["startCharOffset"], 0);
        assert_eq!(wire["hits"][1]["startCharOffset"], 12);
        assert_eq!(wire["hits"][1]["startLine"], 7);
        assert_eq!(wire["hits"][1]["endLine"], 7);
        assert_eq!(wire["hits"][1]["content"], "函数");
        assert_eq!(wire["formattedContext"], formatted_context);
    }

    /// Keep checkpoint metadata paired with the management read instead of an earlier status sample.
    /// 将检查点元数据与管理读取配套，不能使用较早采样的状态。
    /// No inputs or outputs; asserts both heavy-snapshot and lightweight-only projection paths.
    /// 无输入输出；断言管理快照与仅轻量状态两条投影路径。
    #[test]
    fn overview_uses_management_checkpoint_and_workspace_together() {
        // All model routes are available; this fixture performs no model or storage calls.
        // 所有模型路由均可用；本夹具不执行模型或存储调用。
        let route = api::VceConfigurationCapabilityStatus {
            configured: true,
            available: true,
            issue: None,
        };
        let configuration = api::VceGlobalConfigurationStatus {
            ready: true,
            consumer_profile_id: String::from("vce"),
            query_rewrite_purpose_id: String::from("rewrite"),
            reasoning_policy: api::VceReasoningPolicy::FixedOff,
            embedding: route.clone(),
            query_rewrite: route.clone(),
            rerank: route,
            issues: Vec::new(),
        };
        // Status was sampled before a publication completed.
        // 状态采样发生在一次发布完成之前。
        let status = VceRuntimeProjectStatus {
            project_id: String::from("project-a"),
            lifecycle: VceProjectLifecycle::Refreshing,
            config_revision: 1,
            embedding_physical_fingerprint: Some(String::from("fixture-space")),
            engine: Some(vulcan_context_engine::VceEngineSnapshot {
                state: VceEngineState::Ready,
                workspace_generation: 1,
                checkpoint_version: Some(1),
                last_refresh: None,
                last_retrieval: None,
                last_error: None,
            }),
            workspace: Some(vulcan_context_engine::VceWorkspaceStatistics {
                present_files: 2,
                checkpoint_blobs: 2,
                ..Default::default()
            }),
            last_error: None,
        };
        // The management operation subsequently pinned checkpoint two and its one surviving file.
        // 随后的管理操作固定了检查点二及其唯一保留文件。
        let snapshot = vulcan_context_engine::VceManagementSnapshot {
            engine: vulcan_context_engine::VceEngineSnapshot {
                checkpoint_version: Some(2),
                workspace_generation: 2,
                ..status.engine.clone().expect("fixture engine")
            },
            workspace: vulcan_context_engine::VceWorkspaceStatistics {
                present_files: 1,
                checkpoint_blobs: 1,
                ..Default::default()
            },
            query_statistics: Default::default(),
            ai: vulcan_context_engine::VceAiCapabilitySnapshot {
                registration_id: String::from("fixture-registration"),
                consumer_profile_id: String::from("vce"),
                vector_space_id: String::from("fixture-space"),
                registered: true,
                embedding_available: true,
                query_rewrite_available: true,
                rerank_available: true,
            },
            index: None,
        };
        // Lightweight activity still uses its own status and never requires a management scan.
        // 轻量活动仍使用自己的状态，绝不要求管理扫描。
        let settings = vce_settings_summary(&ProjectVceSettings::default());
        let light = vce_overview_from_runtime(
            configuration.clone(),
            settings.clone(),
            status.clone(),
            None,
        );
        assert_eq!(light.status.checkpoint_version, Some(1));
        assert_eq!(light.status.present_files, 2);
        // Heavy overview must project the same checkpoint and counters as its pinned snapshot.
        // 完整概览必须投影与固定快照相同的检查点及计数。
        let overview = vce_overview_from_runtime(configuration, settings, status, Some(snapshot));
        assert_eq!(overview.status.checkpoint_version, Some(2));
        assert_eq!(overview.status.workspace_generation, Some(2));
        assert_eq!(overview.status.present_files, 1);
        assert_eq!(overview.status.checkpoint_blobs, 1);
        assert_eq!(
            overview.status.lifecycle,
            api::VceProjectLifecycleState::Indexing
        );
    }

    /// A delayed full refresh cannot restore revoked project intent or replace unrelated project metadata.
    /// 延迟完整刷新不能恢复已撤销项目意图，也不能替换无关项目元数据。
    /// No inputs or outputs; validates monotonic policy preservation and equal-revision conflict rejection.
    /// 无输入输出；验证单调策略保留及相同修订冲突拒绝。
    #[test]
    fn delayed_workbench_refresh_preserves_newer_vce_authority_only() {
        // The loaded path/title remain the authoritative disk values even when their VCE policy is stale.
        // 即使向量策略已过期，加载的路径及标题仍保持权威磁盘值。
        let mut loaded = vec![TaskProjectSummary::new(
            "project-a",
            "disk title",
            "disk-path",
            false,
            0,
            None,
            None,
            Vec::new(),
        )];
        loaded[0].vce.revision = 4;
        loaded[0].vce.enabled = true;
        let mut retained = loaded.clone();
        retained[0].title = String::from("old title");
        retained[0].path = String::from("old-path");
        retained[0].vce.revision = 5;
        retained[0].vce.enabled = false;
        retained[0].vce.extra_ignore_patterns = vec![String::from("secret/")];
        let mut projected = vec![api::ProjectSummary {
            id: String::from("project-a"),
            name: String::from("disk title"),
            path: String::from("disk-path"),
            pinned: false,
            icon_pack: None,
            icon_key: None,
            vce_enabled: true,
            vce_settings_revision: 4,
        }];
        preserve_newer_vce_project_settings(&mut loaded, &retained, &mut projected).unwrap();
        assert_eq!(loaded[0].vce, retained[0].vce);
        assert_eq!(loaded[0].path, "disk-path");
        assert_eq!(loaded[0].title, "disk title");
        assert!(!projected[0].vce_enabled);
        assert_eq!(projected[0].vce_settings_revision, 5);
        loaded[0].vce.revision = 6;
        loaded[0].vce.enabled = true;
        preserve_newer_vce_project_settings(&mut loaded, &retained, &mut projected).unwrap();
        assert!(projected[0].vce_enabled);
        assert_eq!(projected[0].vce_settings_revision, 6);
        retained[0].vce.revision = 6;
        assert!(
            preserve_newer_vce_project_settings(&mut loaded, &retained, &mut projected).is_err()
        );
        retained[0].id = String::from("unrelated-project");
        preserve_newer_vce_project_settings(&mut loaded, &retained, &mut projected).unwrap();
        assert!(
            loaded[0].vce.enabled,
            "matching titles cannot transfer policy between project IDs"
        );
    }

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

    /// Verifies that an absent checkpoint is reserved for explicit rebuild recovery.
    /// 验证空检查点仅用于明确的重建恢复流程。
    #[test]
    fn maintenance_checkpoint_requirement_is_explicit() {
        assert!(
            validate_maintenance_checkpoint_requirement(VceMaintenanceMode::Rebuild, None).is_ok()
        );
        assert!(
            validate_maintenance_checkpoint_requirement(VceMaintenanceMode::Rebuild, Some(7))
                .is_ok()
        );
        assert!(
            validate_maintenance_checkpoint_requirement(VceMaintenanceMode::Repair, Some(7))
                .is_ok()
        );
        assert!(
            validate_maintenance_checkpoint_requirement(
                VceMaintenanceMode::CleanupStaging,
                Some(7),
            )
            .is_ok()
        );
        assert!(
            validate_maintenance_checkpoint_requirement(VceMaintenanceMode::Compact, Some(7))
                .is_ok()
        );
        assert_eq!(
            validate_maintenance_checkpoint_requirement(VceMaintenanceMode::Repair, None),
            Err(String::from(
                "修复、暂存清理或物理整理要求一个已发布检查点。"
            ))
        );
        assert_eq!(
            validate_maintenance_checkpoint_requirement(VceMaintenanceMode::CleanupStaging, None),
            Err(String::from(
                "修复、暂存清理或物理整理要求一个已发布检查点。"
            ))
        );
        assert_eq!(
            validate_maintenance_checkpoint_requirement(VceMaintenanceMode::Compact, None),
            Err(String::from(
                "修复、暂存清理或物理整理要求一个已发布检查点。"
            ))
        );
    }

    /// Verifies VCE settings input preserves host-owned migration state.
    /// 验证 VCE 设置输入保留宿主持有的迁移状态。
    #[test]
    fn settings_input_preserves_host_owned_fields() {
        let input = api::VceProjectSettingsInput {
            enabled: true,
            update_mode: api::VceIndexUpdateMode::Automatic,
            auto_update_min_interval_seconds: 600,
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
        assert_eq!(
            settings.update_mode,
            vulcan_workbench_model::ProjectVceUpdateMode::Automatic
        );
        assert_eq!(settings.auto_update_min_interval_seconds, 600);
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
        assert!(runtime_status_can_serve_requests(
            Some(&status),
            7,
            "sha256:embedding-a"
        ));
        // The same failure during initial creation cannot expose a missing checkpoint.
        // 首次创建期间相同的失败不能暴露不存在的检查点。
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
    }
}
