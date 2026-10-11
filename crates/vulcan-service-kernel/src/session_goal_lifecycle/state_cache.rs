//! Session Goal V1 projection and retained WebUI cache helpers.
//! 会话 Goal V1 投影与 WebUI 保留缓存辅助逻辑。

use std::path::Path;
use std::sync::Arc;

use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_session_core::v1::config::{
    GoalState, KnowledgeLibrarySelectionConfig, KnowledgeLibrarySelectionMode,
    KnowledgeRetrievalMode, QueuedInput, VmmSessionPolicyConfig,
};
use vulcan_session_core::v1::document::SessionDocument;
use vulcan_session_core::v1::ids::{ResourceId, Seq, TurnId};
use vulcan_workbench_model::{
    SessionKnowledgeLibrarySelection, SessionKnowledgeLibrarySelectionMode,
};

use super::*;
use crate::pending_prompt_services::reconcile_pending_prompt_attachment_leases;
use crate::pending_prompts::pending_prompt_process_mirror_from_v1_queued_inputs;
use crate::session_runtime_registry_v1::SessionHandle;
use crate::session_v1_lifecycle::get_or_load_session_v1;
use crate::terminal_settlement_service::rebuild_pending_terminal_settlement_owner;
use crate::{
    PendingPromptProcessMirror, composer_attachment_workspace_path_from_snapshot,
    current_unix_millis_value, webui_trace,
};
use vulcan_workspace_store::workspace_store::{
    MISSING_RESOURCE_PLACEHOLDER, ProjectResourceCatalog, ResourceCatalogEntry,
    load_project_resource_catalog, locate_session_v1_storage,
};

/// Restart projection derived from one exact V1 document snapshot.
/// 从一份精确 V1 文档快照派生的重启投影。
#[derive(Debug, Clone)]
struct SessionV1RuntimeProjection {
    /// Goal summary derived from V1 metadata and persisted statistics.
    /// 从 V1 metadata 与持久化统计派生的 Goal 摘要。
    goal_summary: Option<SessionGoalSummary>,
    /// Persisted context-compaction window node.
    /// 持久化的上下文压缩窗口节点。
    context_compaction_window_node_id: Option<vulcan_context::ContextCompactionWindowNodeId>,
    /// Persisted output-style override.
    /// 持久化的输出风格覆盖。
    output_style_id: Option<String>,
    /// Whether Plan mode is active.
    /// Plan 模式是否激活。
    plan_enabled: bool,
    /// Provider route currently effective at the last safe API boundary.
    /// 最近安全 API 边界已经生效的 Provider 路由。
    active_provider_id: Option<String>,
    /// Non-secret connection currently effective at the last safe API boundary.
    /// 最近安全 API 边界已经生效的非秘密连接。
    active_connection_id: Option<String>,
    /// Model currently effective at the last safe API boundary.
    /// 最近安全 API 边界已经生效的模型。
    active_model_id: Option<String>,
    /// Exact model route waiting for the next safe API boundary.
    /// 等待下一个安全 API 边界生效的精确模型路由。
    pending_model_selection: Option<vulcan_workbench_model::SessionPendingModelSelection>,
    /// User-selected reasoning effort, preferring the pending next-request value.
    /// 用户选择的思考级别，优先采用待下一请求生效的值。
    selected_reasoning_effort: Option<vulcan_provider_config::config::ReasoningEffortLevel>,
    /// Temperature currently effective at the last safe API boundary.
    /// 最近安全 API 边界已经生效的温度。
    active_temperature: Option<f32>,
    /// V1 metadata update time rendered for the legacy UI adapter.
    /// 为旧 UI 适配器渲染的 V1 metadata 更新时间。
    updated_at: String,
    /// Durable queued inputs in exact FIFO order.
    /// 按精确 FIFO 顺序排列的可靠排队输入。
    queued_inputs: Vec<QueuedInput>,
    /// Complete session-owned Provider tool selection.
    /// 当前会话持有的完整 Provider 工具选择。
    provider_tools: vulcan_session_core::v1::config::ProviderToolSelectionConfig,
    /// Complete session-owned knowledge-library selection.
    /// 当前会话持有的完整知识库选择。
    knowledge_libraries: KnowledgeLibrarySelectionConfig,
    /// Complete session-owned VMM policy pair.
    /// 当前会话持有的完整 VMM 策略对。
    vmm: VmmSessionPolicyConfig,
}

/// Projects all Stage 6 restart facts from one immutable V1 document view.
/// 从一份不可变 V1 文档视图投影全部阶段 6 重启事实。
fn session_v1_runtime_projection(
    document: &SessionDocument,
    session_id: SessionId,
) -> SessionV1RuntimeProjection {
    SessionV1RuntimeProjection {
        goal_summary: session_goal_summary_from_document(document, session_id, None),
        context_compaction_window_node_id: document
            .metadata
            .config
            .context_compaction_window_node_id,
        output_style_id: document.metadata.config.selection.output_style.clone(),
        plan_enabled: document.metadata.mode.plan.enabled,
        active_provider_id: document.metadata.config.active_request.provider.clone(),
        active_connection_id: document
            .metadata
            .config
            .active_request
            .connection_id
            .clone(),
        active_model_id: document.metadata.config.active_request.model.clone(),
        pending_model_selection: pending_model_selection_from_v1(document),
        selected_reasoning_effort: selected_reasoning_effort_from_v1(document),
        active_temperature: legacy_temperature_from_v1(
            document.metadata.config.active_request.temperature,
        ),
        updated_at: document.metadata.updated_at_ms.to_string(),
        queued_inputs: document.metadata.input_state.queued_inputs.clone(),
        provider_tools: document.metadata.config.selection.provider_tools.clone(),
        knowledge_libraries: document
            .metadata
            .config
            .selection
            .knowledge_libraries
            .clone(),
        vmm: document.metadata.config.selection.vmm.clone(),
    }
}

/// Projects a complete pending V1 route into the temporary legacy WebUI adapter.
/// 把完整的 V1 待生效路由投影到阶段内旧 WebUI 适配器。
fn pending_model_selection_from_v1(
    document: &SessionDocument,
) -> Option<vulcan_workbench_model::SessionPendingModelSelection> {
    let pending = document.metadata.config.pending_request.as_ref()?;
    Some(vulcan_workbench_model::SessionPendingModelSelection {
        provider_id: pending.provider.clone()?,
        connection_id: pending.connection_id.clone()?,
        model_id: pending.model.clone()?,
    })
}

/// Projects the user-selected V1 reasoning level without changing its safe-boundary semantics.
/// 投影用户选择的 V1 思考级别，同时不改变其安全边界语义。
fn selected_reasoning_effort_from_v1(
    document: &SessionDocument,
) -> Option<vulcan_provider_config::config::ReasoningEffortLevel> {
    document
        .metadata
        .config
        .pending_request
        .as_ref()
        .and_then(|pending| pending.thinking_level.as_deref())
        .or(document
            .metadata
            .config
            .active_request
            .thinking_level
            .as_deref())
        .and_then(crate::parse_reasoning_effort)
}

/// Narrows a finite V1 temperature for the temporary legacy `f32` UI field.
/// 把有限的 V1 温度收窄到阶段内旧 UI 使用的 `f32` 字段。
fn legacy_temperature_from_v1(temperature: Option<f64>) -> Option<f32> {
    temperature
        .and_then(|value| value.to_string().parse::<f32>().ok())
        .filter(|value| value.is_finite())
}

/// Applies V1 restart facts to the temporary legacy WebUI runtime adapter.
/// 将 V1 重启事实应用到阶段内旧 WebUI 运行态适配器。
fn apply_session_v1_runtime_projection(
    runtime_state: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
    projection: &SessionV1RuntimeProjection,
) {
    runtime_state.context_compaction_window_node_id = projection.context_compaction_window_node_id;
    runtime_state.output_style.override_style_id = projection.output_style_id.clone();
    if projection.plan_enabled {
        runtime_state.active_work_mode = Some(String::from("plan"));
    } else if runtime_state.active_work_mode.as_deref() == Some("plan") {
        // V1 currently owns only the Plan switch; clearing it must not erase independent build/review UI modes.
        // V1 当前只持有 Plan 开关；关闭 Plan 时不得抹除独立的 build/review UI 模式。
        runtime_state.active_work_mode = None;
    }
    runtime_state
        .active_provider_id
        .clone_from(&projection.active_provider_id);
    runtime_state
        .active_connection_id
        .clone_from(&projection.active_connection_id);
    runtime_state
        .active_model_id
        .clone_from(&projection.active_model_id);
    runtime_state
        .pending_model_selection
        .clone_from(&projection.pending_model_selection);
    runtime_state.active_reasoning_effort = projection.selected_reasoning_effort;
    runtime_state.active_temperature = projection.active_temperature;
    crate::session_request_config_v1::apply_provider_tool_selection_v1(
        runtime_state,
        &projection.provider_tools,
    );
    runtime_state.knowledge_library_selection = SessionKnowledgeLibrarySelection {
        mode: match projection.knowledge_libraries.mode {
            KnowledgeLibrarySelectionMode::Inherit => SessionKnowledgeLibrarySelectionMode::Inherit,
            KnowledgeLibrarySelectionMode::Explicit => {
                SessionKnowledgeLibrarySelectionMode::Explicit
            }
        },
        library_ids: projection.knowledge_libraries.library_ids.clone(),
        retrieval_mode: projection
            .knowledge_libraries
            .retrieval_mode
            .map(|mode| match mode {
                KnowledgeRetrievalMode::RawBm25 => String::from("raw_bm25"),
                KnowledgeRetrievalMode::DisplayBm25 => String::from("display_bm25"),
                KnowledgeRetrievalMode::DisplayHybrid => String::from("display_hybrid"),
            }),
    };
    runtime_state.vmm.precheck_enabled = projection.vmm.precheck_enabled;
    runtime_state.vmm.memory_writeback_enabled = projection.vmm.memory_writeback_enabled;
    runtime_state.updated_at.clone_from(&projection.updated_at);
}

/// Creates or updates the temporary WebUI adapter directly from one V1 projection.
/// 直接根据一份 V1 投影创建或更新阶段内 WebUI 适配器。
///
/// # Parameters
/// 参数
/// `runtime_states` stores process-local presentation adapters and is never a persistence authority.
/// `runtime_states` 保存进程内展示适配器，永远不是持久化权威。
/// `session_id` identifies the exact V1 session whose adapter is projected.
/// `session_id` 标识需要投影适配器的精确 V1 会话。
/// `projection` contains restart facts read from the validated V1 document.
/// `projection` 包含从已校验 V1 文档读取的重启事实。
fn upsert_session_v1_runtime_projection(
    runtime_states: &mut std::collections::BTreeMap<
        SessionId,
        vulcan_workbench_model::SessionRuntimeStateEntry,
    >,
    session_id: SessionId,
    projection: &SessionV1RuntimeProjection,
) {
    let runtime_state = runtime_states.entry(session_id).or_default();
    apply_session_v1_runtime_projection(runtime_state, projection);
}

/// Returns the current Unix millisecond timestamp in the V1 domain width.
/// 以 V1 领域要求的位宽返回当前 Unix 毫秒时间戳。
///
/// # Returns
/// 返回值
/// Returns the checked timestamp or an explicit range error.
/// 返回经过检查的时间戳，越界时返回显式错误。
pub(crate) fn current_v1_goal_unix_millis() -> Result<i64, String> {
    i64::try_from(current_unix_millis_value())
        .map_err(|_| String::from("Current Unix millisecond timestamp exceeds the V1 domain range"))
}

/// Loads the unique V1 session handle through the process registry.
/// 通过进程注册表加载唯一的 V1 会话句柄。
///
/// # Parameters
/// 参数
/// `state` owns the controller and registry; `session_id` identifies the exact indexed session.
/// `state` 持有控制器与注册表；`session_id` 标识精确索引的会话。
///
/// # Returns
/// 返回值
/// Returns the unique registered handle or an explicit indexed-load failure.
/// 返回唯一已注册句柄，或显式的精确索引加载失败。
pub(crate) async fn session_v1_handle_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Arc<SessionHandle>, String> {
    let (controller_runtime, registry) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        (
            controller_runtime,
            Arc::clone(&guard.session_runtime_registry),
        )
    };
    get_or_load_session_v1(&controller_runtime, registry.as_ref(), session_id)
        .await
        .map_err(|error| format!("Failed to load V1 session `{session_id}`: {error}"))
}

/// Loads one V1 session handle and re-arms any durable terminal settlement owner it still reports.
/// 加载一个 V1 会话句柄，并重新武装其仍然报告的可靠终态结算 owner。
///
/// Once a settlement intent is durable, restart re-arms its admission block from the document;
/// one controlled retry can then settle the exact turn idempotently. A failure before that intent
/// becomes durable has only the separate process-local repair path.
/// 结算意图可靠落盘后，重启会从文档重新武装其准入阻塞；一次受控重试即可幂等结算精确回合。
/// 意图落盘前的失败仅有独立的进程内修复路径。
pub(crate) async fn session_v1_handle_from_state_with_settlement_rebuild(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Arc<SessionHandle>, String> {
    let handle = session_v1_handle_from_state(state, session_id).await?;
    rebuild_pending_terminal_settlement_owner(state, session_id, handle.as_ref());
    Ok(handle)
}

/// Returns the already-hydrated V1 session handle without consulting legacy state or storage.
/// 返回已完成 hydration 的 V1 会话句柄，不读取旧状态或存储。
///
/// # Parameters
/// 参数
/// `state` owns the process registry and `session_id` selects the exact registered session.
/// `state` 持有进程注册表，`session_id` 选择精确的已注册会话。
///
/// # Returns
/// 返回值
/// Returns the unique registered handle, or an explicit lifecycle-ordering failure.
/// 返回唯一已注册句柄；生命周期顺序错误时返回显式失败。
pub(crate) fn registered_session_v1_handle_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Arc<SessionHandle>, String> {
    let registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        Arc::clone(&guard.session_runtime_registry)
    };
    registered_session_v1_handle(&registry, session_id)
}

/// Returns the already-hydrated V1 session handle from a registry the caller already owns.
/// 从调用方已持有的注册表返回已完成 hydration 的 V1 会话句柄。
///
/// Callers that run while holding another lock use this form so the lookup never waits on the
/// service state lock.
/// 在持有其他锁时运行的调用方使用此形式，使查找永不等待服务状态锁。
///
/// # Parameters
/// 参数
/// `registry` is the process session registry and `session_id` selects the exact registered session.
/// `registry` 是进程会话注册表，`session_id` 选择精确的已注册会话。
///
/// # Returns
/// 返回值
/// Returns the unique registered handle, or an explicit lifecycle-ordering failure.
/// 返回唯一已注册句柄；生命周期顺序错误时返回显式失败。
pub(crate) fn registered_session_v1_handle(
    registry: &crate::session_runtime_registry_v1::SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<Arc<SessionHandle>, String> {
    registry.get(session_id).map_err(|error| {
        format!("V1 session `{session_id}` has not finished exact hydration: {error}")
    })
}

/// Updates the non-authoritative in-memory Goal summary projection.
/// 更新非权威的内存 Goal 摘要投影。
pub(crate) fn cache_session_goal_summary(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    summary: Option<SessionGoalSummary>,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    match summary {
        Some(summary) => {
            guard.session_goal_summaries.insert(session_id, summary);
        }
        None => {
            guard.session_goal_summaries.remove(&session_id);
        }
    }
    Ok(())
}

/// Reads Goal metadata and usage only from the authoritative V1 document.
/// 仅从权威 V1 文档读取 Goal metadata 与用量。
///
/// # Parameters
/// 参数
/// `handle` owns the authoritative document; `session_id` is its externally visible identity.
/// `handle` 持有权威文档；`session_id` 是其对外可见身份。
///
/// # Returns
/// 返回值
/// Returns no summary when Goal mode is disabled, otherwise one stats-derived projection.
/// Goal 模式未启用时返回空，否则返回一份由统计事实派生的投影。
pub(crate) fn session_goal_summary_from_v1_handle(
    handle: &SessionHandle,
    session_id: SessionId,
) -> Result<Option<SessionGoalSummary>, String> {
    handle
        .read(|document| {
            session_goal_summary_from_document(
                document,
                session_id,
                handle.goal_usage_snapshot().ok().as_ref(),
            )
        })
        .map_err(|error| format!("Failed to read the V1 goal state: {error}"))
}

/// Durable Goal dispatch identity captured from one authoritative V1 document read.
/// 从一次权威 V1 文档读取中捕获的可靠 Goal 分发身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionGoalDispatchAuthority {
    /// Exact history generation; local turn and sequence identifiers may repeat after replacement.
    /// 精确历史代次；替换后局部回合与序号标识可能重复。
    pub(crate) generation: u64,
    /// Stable projected Goal stage identity.
    /// 稳定投影的 Goal 阶段身份。
    pub(crate) goal_id: String,
    /// Exact persisted open Goal-continuation Turn.
    /// 精确持久化的开放 Goal 续跑回合。
    pub(crate) turn_id: TurnId,
    /// Exact Provider-visible context seq that opened the continuation Turn.
    /// 开启续跑回合的精确 Provider 可见上下文 seq。
    pub(crate) context_seq: Seq,
}

/// Reads the active Goal and its open continuation Turn as one lock-consistent fact.
/// 以一次锁一致事实读取活动 Goal 及其开放续跑回合。
///
/// # Parameters
/// 参数
/// `handle` owns the authoritative V1 document and `session_id` identifies its service projection.
/// `handle` 持有权威 V1 文档，`session_id` 标识其服务投影。
///
/// # Returns
/// 返回值
/// Returns the exact durable dispatch identity only when Goal is active and its continuation Turn remains open.
/// 仅当 Goal 处于活动状态且其续跑回合仍开放时返回精确可靠分发身份。
pub(crate) fn session_goal_dispatch_authority_from_v1_handle(
    handle: &SessionHandle,
    session_id: SessionId,
) -> Result<Option<SessionGoalDispatchAuthority>, String> {
    handle
        .read(|document| session_goal_dispatch_authority_from_document(document, session_id))
        .map_err(|error| format!("Failed to read the V1 goal dispatch identity: {error}"))
}

/// Derives Goal dispatch identity from one caller-owned canonical snapshot.
/// 从调用方持有的一份规范快照推导 Goal 分发身份。
/// Returns no authority while stopped, archived, or occupied; accounting does not grant execution.
/// 停止、归档或被占用时不返回权威；统计不授予执行权限。
pub(crate) fn session_goal_dispatch_authority_from_document(
    document: &SessionDocument,
    session_id: SessionId,
) -> Option<SessionGoalDispatchAuthority> {
    // Goal identity and lifecycle are canonical; this projection never needs mutable usage counters.
    // Goal 身份与生命周期由规范记录决定；此投影不需要可变使用量计数。
    let summary = session_goal_summary_from_document(document, session_id, None)?;
    if summary.status != SessionGoalStatus::Active
        || document.metadata.archived_at_ms.is_some()
        || document.collaboration_run_control()
            != vulcan_session_core::v1::collaboration::CollaborationRunControl::Running
        || document.active_api().is_some()
        || document.collaboration_state().question_executions.values().any(|execution| {
            execution.state == vulcan_session_core::v1::collaboration::CollaborationQuestionExecutionState::Running
        })
    {
        return None;
    }
    // Require the actual Goal continuation owner, not merely active Goal metadata on a user turn.
    // 要求实际 Goal 续跑所有者，而非仅附着于用户回合的活动 Goal 元数据。
    let turn = document.active_turn()?;
    (!turn.closed && turn.origin == vulcan_session_core::v1::document::TurnOrigin::GoalContinuation)
        .then_some(SessionGoalDispatchAuthority {
            generation: document.segment_generation(),
            goal_id: summary.goal_id,
            turn_id: turn.turn_id,
            context_seq: turn.start_seq,
        })
}

/// Maps one catalog MIME value to the stable conversation attachment category.
/// 将一项目录 MIME 值映射为稳定的会话附件类别。
fn queued_resource_attachment_kind(mime: Option<&str>) -> ConversationAttachmentKind {
    let normalized = mime.unwrap_or_default().trim().to_ascii_lowercase();
    if normalized.starts_with("image/") {
        ConversationAttachmentKind::Image
    } else if normalized.starts_with("video/") {
        ConversationAttachmentKind::Video
    } else if normalized.starts_with("audio/") {
        ConversationAttachmentKind::Audio
    } else if normalized == "application/pdf" {
        ConversationAttachmentKind::Pdf
    } else {
        ConversationAttachmentKind::File
    }
}

/// Builds the fixed non-blocking UI placeholder for one missing project resource.
/// 为一项缺失的项目资源构建固定且不阻断 UI 的占位附件。
fn missing_queued_resource_attachment(_resource_id: ResourceId) -> ConversationAttachment {
    ConversationAttachment {
        kind: ConversationAttachmentKind::File,
        file_name: MISSING_RESOURCE_PLACEHOLDER.to_string(),
        local_path: MISSING_RESOURCE_PLACEHOLDER.to_string(),
        resource_path: None,
        source_path: None,
        asset_id: None,
        byte_len: None,
        source_sha256: None,
        delivery_mode: None,
        preview_local_path: None,
        mime_type: String::from("application/octet-stream"),
    }
}

/// Projects one exact catalog entry into the process-local queue attachment shape.
/// 将一项精确目录条目投影为进程内队列附件结构。
fn queued_resource_attachment(
    project_directory: &Path,
    entry: &ResourceCatalogEntry,
) -> ConversationAttachment {
    let entity_path = project_directory.join(&entry.entity_relative_path);
    ConversationAttachment {
        kind: queued_resource_attachment_kind(entry.mime.as_deref()),
        file_name: entry.original_name.clone(),
        local_path: entity_path.to_string_lossy().into_owned(),
        resource_path: Some(entry.entity_relative_path.clone()),
        source_path: None,
        asset_id: None,
        byte_len: Some(entry.size),
        source_sha256: Some(entry.sha256.clone()),
        delivery_mode: None,
        preview_local_path: None,
        mime_type: entry
            .mime
            .clone()
            .unwrap_or_else(|| String::from("application/octet-stream")),
    }
}

/// Resolves ordered project resource ids into process-local conversation attachments.
/// 在不扫描项目存储的情况下，把有序项目资源 ID 解析为进程内会话附件。
pub(crate) fn conversation_attachments_from_resource_ids(
    project_directory: &Path,
    catalog: &ProjectResourceCatalog,
    resource_ids: &[ResourceId],
) -> Vec<ConversationAttachment> {
    resource_ids
        .iter()
        .map(|resource_id| {
            let Some(entry) = catalog
                .entries
                .iter()
                .find(|entry| entry.resource_id == *resource_id)
            else {
                return missing_queued_resource_attachment(*resource_id);
            };
            let entity_path = project_directory.join(&entry.entity_relative_path);
            if !entity_path.is_file() {
                return missing_queued_resource_attachment(*resource_id);
            }
            queued_resource_attachment(project_directory, entry)
        })
        .collect()
}

/// Rebuilds selected-session Goal and temporary WebUI runtime projections from exact V1 metadata.
/// 从精确 V1 metadata 重建选中会话 Goal 与阶段内 WebUI 运行态投影。
pub(crate) async fn sync_selected_session_v1_projection_from_store(
    state: &SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
    selected_session_id: SessionId,
    refreshed_snapshot: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
) -> Result<BTreeMap<SessionId, SessionGoalSummary>, String> {
    let projection_and_mirror = if selected_session_id == SessionId::nil() {
        None
    } else {
        let handle = session_v1_handle_from_state(state, selected_session_id).await?;
        let has_pending_release = handle
            .read(|document| {
                !document
                    .metadata
                    .input_state
                    .pending_staging_release_actions
                    .is_empty()
            })
            .map_err(|error| {
                format!("Failed to read the V1 queued attachment release state: {error}")
            })?;
        if has_pending_release {
            let attachment_workspace_path = composer_attachment_workspace_path_from_snapshot(
                refreshed_snapshot,
                None,
                Some(&selected_session_id.to_string()),
            );
            let attachment_reconcile = attachment_workspace_path.and_then(|workspace_path| {
                reconcile_pending_prompt_attachment_leases(
                    handle.as_ref(),
                    selected_session_id,
                    &workspace_path,
                )
            });
            if let Err(error) = attachment_reconcile {
                webui_trace(
                    "pending_prompt_attachment_lease_reconcile_failed",
                    format!("session={selected_session_id} error={error}"),
                );
            }
        }
        let mut projection = handle
            .read(|document| session_v1_runtime_projection(document, selected_session_id))
            .map_err(|error| {
                format!("Failed to read the V1 session configuration projection: {error}")
            })?;
        projection.goal_summary =
            session_goal_summary_from_v1_handle(&handle, selected_session_id)?;
        let process_mirror = if projection.queued_inputs.is_empty() {
            PendingPromptProcessMirror::default()
        } else {
            let selected_session_id_text = selected_session_id.to_string();
            let locator =
                locate_session_v1_storage(&controller_runtime, selected_session_id_text.as_str())
                    .await
                    .map_err(|error| {
                        format!("Failed to locate the project of the V1 queued resources: {error}")
                    })?;
            let project_directory = locator.project_directory();
            let catalog = load_project_resource_catalog(&project_directory).map_err(|error| {
                format!("Failed to load the V1 queued resource catalog: {error}")
            })?;
            pending_prompt_process_mirror_from_v1_queued_inputs(
                &projection.queued_inputs,
                |queued_input| {
                    Ok(conversation_attachments_from_resource_ids(
                        &project_directory,
                        &catalog,
                        &queued_input.resource_ids,
                    ))
                },
            )?
        };
        Some((projection, process_mirror))
    };
    if let Some((projection, process_mirror)) = projection_and_mirror {
        // These legacy fields are in-memory WebUI adapters only; the exact restart values come from V1 metadata.
        // 这些旧字段仅是内存 WebUI 适配；重启后的精确值全部来自 V1 metadata。
        upsert_session_v1_runtime_projection(
            &mut refreshed_snapshot.session_runtime_state,
            selected_session_id,
            &projection,
        );
        cache_session_goal_summary(state, selected_session_id, projection.goal_summary)?;
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if process_mirror
            .ui_state
            .pending_prompt_panel_queue
            .is_empty()
        {
            guard
                .pending_prompt_process_mirrors
                .remove(&selected_session_id);
        } else {
            guard
                .pending_prompt_process_mirrors
                .insert(selected_session_id, process_mirror);
        }
    }
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(guard.session_goal_summaries.clone())
}

/// Projects one V1 Goal configuration and its persisted statistics to the service DTO.
/// 将一份 V1 Goal 配置及其持久化统计投影为服务 DTO。
fn session_goal_summary_from_document(
    document: &SessionDocument,
    session_id: SessionId,
    current_usage: Option<&vulcan_session_core::v1::lifetime_usage::GoalUsageSnapshot>,
) -> Option<SessionGoalSummary> {
    // Only Goal clock lifecycle facts remain event-derived; token accounting uses the frozen baseline.
    // 仅 Goal 时钟生命周期仍来自事件；Token 统计使用冻结基线。
    let goal = &document.metadata.mode.goal;
    if !goal.enabled {
        return None;
    }
    let delta = current_usage.and_then(|current| current.since(goal.usage_baseline.as_ref()?));
    let known_budget = delta.as_ref().and_then(|usage| usage.known_budget());
    let usage = delta.unwrap_or_default();
    let work_time = vulcan_session_core::v1::goal::goal_work_time(document);
    let created_at_ms = goal
        .origin_seq
        .and_then(|seq| {
            document
                .events()
                .iter()
                .find(|event| event.seq == seq)
                .map(|event| event.created_at_ms)
        })
        .unwrap_or(document.metadata.created_at_ms);
    Some(SessionGoalSummary {
        session_id: session_id.to_string(),
        goal_id: format!("goal-v1-{}", goal.origin_seq.map_or(0, Seq::value)),
        objective: goal.objective.clone(),
        status: session_goal_status_from_goal_state(goal.state),
        token_budget: goal.budget_tokens.map(saturating_u64_to_i64),
        usage_known: known_budget.is_some(),
        tokens_used: saturating_u64_to_i64(known_budget.unwrap_or_default()),
        usage_tokens: saturating_u64_to_i64(known_budget.unwrap_or_default()),
        api_call_count: saturating_u64_to_usize(usage.request_count),
        tool_call_count: saturating_u64_to_usize(usage.tool_call_count),
        input_tokens: saturating_u64_to_i64(usage.uncached_input_tokens.sum),
        cached_input_tokens: saturating_u64_to_i64(usage.cache_read_tokens.sum),
        output_tokens: saturating_u64_to_i64(usage.output_tokens.sum),
        reasoning_tokens: saturating_u64_to_i64(usage.reasoning_tokens.sum),
        total_tokens: saturating_u64_to_i64(usage.total_tokens.sum),
        time_used_seconds: saturating_u64_to_i64(
            work_time.map_or(0, |clock| clock.elapsed_ms / 1000),
        ),
        work_time: work_time.map(|clock| vulcan_service_api::SessionGoalWorkTime {
            elapsed_ms: clock.elapsed_ms,
            observed_at_ms: clock.observed_at_ms,
        }),
        created_at_ms,
        updated_at_ms: document.metadata.updated_at_ms,
    })
}

/// Converts a V1 Goal state into the stable service protocol status.
/// 将 V1 Goal 状态转换为稳定服务协议状态。
pub(crate) const fn session_goal_status_from_goal_state(state: GoalState) -> SessionGoalStatus {
    match state {
        GoalState::Active => SessionGoalStatus::Active,
        GoalState::Paused => SessionGoalStatus::Paused,
        GoalState::Blocked => SessionGoalStatus::Blocked,
        GoalState::BudgetExhausted => SessionGoalStatus::BudgetLimited,
        GoalState::Completed => SessionGoalStatus::Complete,
    }
}

/// Saturates an unsigned counter into the signed service DTO range.
/// 将无符号计数饱和转换到服务 DTO 的有符号范围。
const fn saturating_u64_to_i64(value: u64) -> i64 {
    if value > i64::MAX as u64 {
        i64::MAX
    } else {
        value as i64
    }
}

/// Saturates an unsigned counter into the platform-sized DTO count range.
/// 将无符号计数饱和转换到平台大小的 DTO 计数范围。
const fn saturating_u64_to_usize(value: u64) -> usize {
    if value > usize::MAX as u64 {
        usize::MAX
    } else {
        value as usize
    }
}

#[cfg(test)]
mod tests {
    //! V1 Goal projection and durable dispatch authority tests.
    //! V1 Goal 投影与可靠分发权威测试。

    use super::*;
    use crate::session_runtime_registry_v1::SessionPersistenceBackend;
    use vulcan_session_core::v1::config::GoalConfig;
    use vulcan_session_core::v1::document::{
        ClientInputSubmission, FlushCandidate, PlanModeTransitionInput,
    };
    use vulcan_session_core::v1::ids::{ClientActionId, PendingInputId};
    use vulcan_workspace_store::workspace_store::{SessionV1FlushResult, SessionV1StoreError};

    /// Read-only backend proving authority reads never need a persistence side effect.
    /// 只读后端，用于证明权威读取不需要任何持久化副作用。
    #[derive(Debug)]
    struct ReadOnlyPersistenceBackend;

    impl SessionPersistenceBackend for ReadOnlyPersistenceBackend {
        /// Rejects unexpected writes from read-only projection tests.
        /// 拒绝只读投影测试中的意外写入。
        fn flush(
            &self,
            _candidate: &FlushCandidate,
        ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
            panic!("read-only Goal authority projection must not flush")
        }
    }

    /// Builds one handle whose Goal metadata is active and whose open Turn has the requested origin.
    /// 构建一份 Goal metadata 活动且开放回合具有指定来源的句柄。
    fn goal_handle_with_open_turn(
        session_id: SessionId,
        goal_continuation: bool,
        goal_state: GoalState,
    ) -> (Arc<SessionHandle>, TurnId) {
        let mut document = SessionDocument::new(session_id.to_string(), "project", "title", 1);
        document.metadata.mode.goal = GoalConfig {
            enabled: true,
            objective: String::from("finish the migration"),
            state: goal_state,
            ..GoalConfig::default()
        };
        let turn_id = if goal_continuation {
            document
                .start_goal_continuation_turn("finish the migration", 2)
                .expect("Goal continuation Turn should start")
        } else {
            document
                .submit_root_input(
                    ClientInputSubmission::new(
                        ClientActionId::try_new("state-cache-root")
                            .expect("state-cache root action id should be valid"),
                        "user input",
                        Vec::new(),
                    ),
                    2,
                )
                .expect("root Turn should start")
                .into_value()
        };
        document.metadata.mode.goal.origin_turn_id = Some(turn_id);
        document.metadata.mode.goal.origin_seq = document.active_turn().map(|turn| turn.start_seq);
        // This projection fixture models an admitted user activation, including its execution consent.
        // 此投影夹具模拟已准入用户激活，包含其执行许可。
        document
            .set_collaboration_run_control(
                vulcan_session_core::v1::collaboration::SetRunControlInput {
                    operation_id:
                        vulcan_session_core::v1::collaboration::CollaborationOperationId::try_new(
                            "state-cache-goal-running",
                        )
                        .unwrap(),
                    state: vulcan_session_core::v1::collaboration::CollaborationRunControl::Running,
                    changed_at_ms: 3,
                },
            )
            .unwrap();
        (
            SessionHandle::with_backend(session_id, document, Arc::new(ReadOnlyPersistenceBackend)),
            turn_id,
        )
    }

    /// Verifies only an active Goal with the exact open Goal-continuation Turn grants dispatch.
    /// 验证仅活动 Goal 与精确开放 Goal 续跑回合能够授予分发权威。
    #[test]
    fn goal_dispatch_authority_requires_the_open_goal_continuation_turn() {
        let session_id = SessionId::from_u128(0x9a);
        let (handle, turn_id) = goal_handle_with_open_turn(session_id, true, GoalState::Active);
        let authority = session_goal_dispatch_authority_from_v1_handle(handle.as_ref(), session_id)
            .expect("Goal authority should project")
            .expect("active Goal continuation should be dispatchable");
        assert_eq!(authority.turn_id, turn_id);

        let other_session_id = SessionId::from_u128(0x9b);
        let (root_handle, _) =
            goal_handle_with_open_turn(other_session_id, false, GoalState::Active);
        assert!(
            session_goal_dispatch_authority_from_v1_handle(root_handle.as_ref(), other_session_id,)
                .expect("root Turn projection should remain readable")
                .is_none()
        );

        for (session_id, goal_state) in [
            (SessionId::from_u128(0x9c), GoalState::Paused),
            (SessionId::from_u128(0x9d), GoalState::Completed),
        ] {
            let (terminal_handle, _) = goal_handle_with_open_turn(session_id, true, goal_state);
            assert!(
                session_goal_dispatch_authority_from_v1_handle(
                    terminal_handle.as_ref(),
                    session_id,
                )
                .expect("terminal Goal projection should remain readable")
                .is_none(),
                "a terminal Goal must not recover its stale open Turn"
            );
        }
    }

    /// Verifies V1 restart projection creates its temporary UI adapter without legacy hydration.
    /// 验证 V1 重启投影无需旧水合即可创建阶段内 UI 适配器。
    #[test]
    fn restart_projection_creates_missing_temporary_runtime_adapter() {
        let session_id = SessionId::from_u128(0x9f);
        let mut document = SessionDocument::new(session_id.to_string(), "project", "title", 1);
        assert!(
            document
                .set_output_style(Some(String::from("concise")), 2)
                .expect("set output style")
        );
        let projection = session_v1_runtime_projection(&document, session_id);
        let mut runtime_states = std::collections::BTreeMap::new();

        upsert_session_v1_runtime_projection(&mut runtime_states, session_id, &projection);

        let runtime_state = runtime_states
            .get(&session_id)
            .expect("V1 projection should create the temporary runtime adapter");
        assert_eq!(
            runtime_state.output_style.override_style_id.as_deref(),
            Some("concise")
        );
        assert_eq!(
            runtime_state.updated_at,
            document.metadata.updated_at_ms.to_string()
        );
    }

    /// Verifies restart hydration derives every Stage 6 WebUI adapter field from V1 metadata.
    /// 验证重启 hydration 从 V1 metadata 派生全部阶段 6 WebUI 适配字段。
    #[test]
    fn restart_projection_restores_stage_six_request_and_mode_facts_from_v1() {
        let session_id = SessionId::from_u128(0x9e);
        let mut document = SessionDocument::new(session_id.to_string(), "project", "title", 1);
        let node_id = "compression-window-4"
            .parse::<vulcan_context::ContextCompactionWindowNodeId>()
            .expect("valid compaction node id");
        assert!(document.set_context_compaction_window_node_id(Some(node_id), 2));
        assert!(
            document
                .set_output_style(Some(String::from("concise")), 3)
                .expect("set output style")
        );
        assert!(
            document
                .set_plan_mode(
                    PlanModeTransitionInput::try_new(true, "plan-enter")
                        .expect("valid Plan context"),
                    4,
                )
                .expect("enable Plan mode")
        );
        document.metadata.config.active_request =
            vulcan_session_core::v1::config::ActiveRequestConfig {
                provider: Some(String::from("provider-a")),
                connection_id: Some(String::from("connection-a")),
                model: Some(String::from("model-a")),
                thinking_level: Some(String::from("medium")),
                temperature: Some(0.5),
            };
        assert!(
            document
                .set_pending_request(
                    vulcan_session_core::v1::config::PendingRequestConfig {
                        provider: Some(String::from("provider-b")),
                        connection_id: Some(String::from("connection-b")),
                        model: Some(String::from("model-b")),
                        thinking_level: Some(String::from("high")),
                        temperature: Some(0.7),
                        set_at_ms: 5,
                    },
                    5,
                )
                .expect("set pending request")
        );
        assert!(
            document
                .set_provider_tool_selection(
                    vulcan_session_core::v1::config::ProviderToolSelectionConfig {
                        initialized: true,
                        overridden: true,
                        enabled_capabilities: vec![String::from("image_understanding")],
                        selections: vec![
                            vulcan_session_core::v1::config::ProviderToolCapabilitySelection {
                                capability: String::from("web_search"),
                                target:
                                    vulcan_session_core::v1::config::ProviderToolTargetConfig {
                                        connection_id: String::from("connection-search"),
                                        target_id: String::from("target-search"),
                                    },
                            },
                        ],
                        media_understanding_selections: vec![
                            vulcan_session_core::v1::config::ProviderMediaUnderstandingSelection {
                                capability: String::from("image_understanding"),
                                mode: vulcan_session_core::v1::config::ProviderMediaUnderstandingMode::ProviderTool,
                                selection: vulcan_session_core::v1::config::ProviderMediaUnderstandingRouteConfig::ProviderTool(
                                        vulcan_session_core::v1::config::ProviderToolTargetConfig {
                                            connection_id: String::from("connection-image"),
                                            target_id: String::from("target-image"),
                                        },
                                    ),
                                reasoning_effort: None,
                            },
                        ],
                        extension_target_sets: vec![
                            vulcan_session_core::v1::config::ProviderExtensionTargetSetConfig {
                                capability: String::from("native_extension"),
                                targets: vec![
                                    vulcan_session_core::v1::config::ProviderToolTargetConfig {
                                        connection_id: String::from("connection-extension"),
                                        target_id: String::from("target-extension"),
                                    },
                                ],
                            },
                        ],
                    },
                    6,
                )
                .expect("set provider tool selection")
        );
        assert!(
            document
                .set_knowledge_library_selection(
                    KnowledgeLibrarySelectionConfig {
                        mode: KnowledgeLibrarySelectionMode::Explicit,
                        library_ids: vec![String::from("library-a")],
                        retrieval_mode: Some(KnowledgeRetrievalMode::DisplayHybrid),
                    },
                    7,
                )
                .expect("set knowledge-library selection")
        );
        assert!(
            document
                .set_vmm_session_policy(
                    VmmSessionPolicyConfig {
                        precheck_enabled: Some(false),
                        memory_writeback_enabled: Some(true),
                    },
                    8,
                )
                .expect("set VMM policy")
        );

        let projection = session_v1_runtime_projection(&document, session_id);
        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry::default();
        apply_session_v1_runtime_projection(&mut runtime_state, &projection);

        assert_eq!(
            runtime_state.context_compaction_window_node_id,
            Some(node_id)
        );
        assert_eq!(
            runtime_state.output_style.override_style_id.as_deref(),
            Some("concise")
        );
        assert_eq!(runtime_state.active_work_mode.as_deref(), Some("plan"));
        assert_eq!(
            runtime_state.active_provider_id.as_deref(),
            Some("provider-a")
        );
        assert_eq!(
            runtime_state.active_connection_id.as_deref(),
            Some("connection-a")
        );
        assert_eq!(runtime_state.active_model_id.as_deref(), Some("model-a"));
        assert_eq!(
            runtime_state.pending_model_selection.as_ref(),
            Some(&vulcan_workbench_model::SessionPendingModelSelection {
                provider_id: String::from("provider-b"),
                connection_id: String::from("connection-b"),
                model_id: String::from("model-b"),
            })
        );
        assert_eq!(
            runtime_state.active_reasoning_effort,
            Some(vulcan_provider_config::config::ReasoningEffortLevel::High)
        );
        assert_eq!(runtime_state.active_temperature, Some(0.5));
        assert_eq!(runtime_state.provider_tool_selections.len(), 1);
        assert_eq!(
            runtime_state.provider_tool_selections.get("web_search"),
            Some(
                &vulcan_workbench_model::SessionProviderToolTargetSelection {
                    connection_id: String::from("connection-search"),
                    target_id: String::from("target-search"),
                }
            )
        );
        assert_eq!(
            runtime_state.provider_tool_enabled_capabilities,
            [String::from("image_understanding")].into_iter().collect()
        );
        assert_eq!(
            runtime_state
                .media_understanding_selections
                .get("image_understanding"),
            Some(
                &vulcan_workbench_model::SessionMediaUnderstandingSelection::ProviderTool {
                    selection: vulcan_workbench_model::SessionProviderToolTargetSelection {
                        connection_id: String::from("connection-image"),
                        target_id: String::from("target-image"),
                    },
                }
            )
        );
        assert_eq!(
            runtime_state
                .provider_extension_target_sets
                .get("native_extension"),
            Some(
                &[vulcan_workbench_model::SessionProviderToolTargetSelection {
                    connection_id: String::from("connection-extension"),
                    target_id: String::from("target-extension"),
                }]
                .into_iter()
                .collect()
            )
        );
        assert!(runtime_state.provider_tool_settings_initialized);
        assert!(runtime_state.provider_tool_selections_overridden);
        assert_eq!(
            runtime_state.knowledge_library_selection.mode,
            SessionKnowledgeLibrarySelectionMode::Explicit
        );
        assert_eq!(
            runtime_state.knowledge_library_selection.library_ids,
            vec![String::from("library-a")]
        );
        assert_eq!(
            runtime_state
                .knowledge_library_selection
                .retrieval_mode
                .as_deref(),
            Some("display_hybrid")
        );
        assert_eq!(runtime_state.vmm.precheck_enabled, Some(false));
        assert_eq!(runtime_state.vmm.memory_writeback_enabled, Some(true));
        assert_eq!(runtime_state.updated_at, "8");
    }

    /// Verifies a disabled V1 Plan switch only clears stale Plan projection, not unrelated UI modes.
    /// 验证已关闭的 V1 Plan 开关只清理过期 Plan 投影，不会抹除无关 UI 模式。
    #[test]
    fn restart_projection_preserves_non_plan_legacy_ui_mode() {
        let session_id = SessionId::from_u128(0x9f);
        let document = SessionDocument::new(session_id.to_string(), "project", "title", 1);
        let projection = session_v1_runtime_projection(&document, session_id);
        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
            active_work_mode: Some(String::from("review")),
            ..vulcan_workbench_model::SessionRuntimeStateEntry::default()
        };

        apply_session_v1_runtime_projection(&mut runtime_state, &projection);

        assert_eq!(runtime_state.active_work_mode.as_deref(), Some("review"));
        runtime_state.active_work_mode = Some(String::from("plan"));
        apply_session_v1_runtime_projection(&mut runtime_state, &projection);
        assert_eq!(runtime_state.active_work_mode, None);
    }

    /// Verifies restart attachment projection follows queued ResourceId order and marks missing entities.
    /// 验证重启附件投影遵循排队 ResourceId 顺序并标记缺失实体。
    #[test]
    fn restart_queue_attachment_projection_preserves_order_and_missing_placeholders() {
        let project_directory = tempfile::tempdir().expect("temporary project should be created");
        let first_path = project_directory.path().join("resources/first.bin");
        let second_path = project_directory.path().join("resources/second.bin");
        std::fs::create_dir_all(
            first_path
                .parent()
                .expect("resource fixture should have a parent directory"),
        )
        .expect("resource directory should be created");
        std::fs::write(&first_path, b"first").expect("first resource should be written");
        std::fs::write(&second_path, b"second").expect("second resource should be written");
        let first_resource_id = ResourceId::try_new(1).expect("first resource id should be valid");
        let second_resource_id =
            ResourceId::try_new(2).expect("second resource id should be valid");
        let missing_resource_id =
            ResourceId::try_new(3).expect("missing resource id should be valid");
        let catalog = ProjectResourceCatalog {
            // Catalog insertion order intentionally differs from queued order.
            // 目录插入顺序故意与排队顺序不同。
            entries: vec![
                ResourceCatalogEntry {
                    resource_id: first_resource_id,
                    original_name: String::from("first.bin"),
                    mime: Some(String::from("application/octet-stream")),
                    size: 5,
                    sha256: String::from("first-sha"),
                    entity_relative_path: String::from("resources/first.bin"),
                    provider_bindings: Vec::new(),
                },
                ResourceCatalogEntry {
                    resource_id: second_resource_id,
                    original_name: String::from("second.bin"),
                    mime: Some(String::from("image/png")),
                    size: 6,
                    sha256: String::from("second-sha"),
                    entity_relative_path: String::from("resources/second.bin"),
                    provider_bindings: Vec::new(),
                },
            ],
            next_resource_id: 4,
        };
        let queued_input = QueuedInput {
            parameters: None,
            bound_request_config: None,
            client_action_id: ClientActionId::try_new("queued-resource-order")
                .expect("client action should be valid"),
            input_id: PendingInputId::try_new(7).expect("pending input id should be valid"),
            text: String::from("inspect resources"),
            resource_ids: vec![second_resource_id, missing_resource_id, first_resource_id],
            created_at_ms: 7,
        };

        let attachments = conversation_attachments_from_resource_ids(
            project_directory.path(),
            &catalog,
            &queued_input.resource_ids,
        );

        assert_eq!(attachments.len(), 3);
        assert_eq!(attachments[0].file_name, "second.bin");
        assert_eq!(attachments[0].kind, ConversationAttachmentKind::Image);
        assert_eq!(attachments[1].file_name, MISSING_RESOURCE_PLACEHOLDER);
        assert_eq!(attachments[1].local_path, MISSING_RESOURCE_PLACEHOLDER);
        assert_eq!(attachments[2].file_name, "first.bin");
    }
}
