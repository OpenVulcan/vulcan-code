//! Human knowledge lifecycle RPCs; AI content mutation remains in the four native tools.
//! 人工知识生命周期 RPC；AI 内容变更保持在四个原生工具中。

use super::*;
use crate::session_goal_lifecycle::{current_v1_goal_unix_millis, session_v1_handle_from_state};
use crate::session_request_config_v1::set_knowledge_library_selection_v1;
use serde::Serialize;
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};
use vulcan_ai_contract::{
    AiOperation, EmbeddingEncoding, EmbeddingOutputKind, EmbeddingTask, HostAiPort,
    InferenceUsageScope,
};
use vulcan_provider_adapters::{InferenceRouteBinding, InferenceRouteConfiguration};
use vulcan_service_api::*;
use vulcan_session_core::v1::config::{
    KnowledgeLibrarySelectionConfig as KnowledgeLibrarySelectionConfigV1,
    KnowledgeLibrarySelectionMode as KnowledgeLibrarySelectionModeV1,
};
use vulcan_workbench_model::{
    SessionKnowledgeLibrarySelection, SessionKnowledgeLibrarySelectionMode,
};
use vulcan_workspace_store::knowledge_base::*;

mod embedding;
mod session;
mod settings;
pub(crate) use embedding::start_knowledge_workers;
pub(crate) use session::{
    service_get_session_knowledge_libraries, service_set_session_knowledge_libraries,
};
use settings::knowledge_settings_from_request;

/// Serializes an authoritative value; returns English diagnostics without an alternate result shape.
/// 序列化权威值；返回英文诊断，不提供其他结果形状。
fn knowledge_json(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value)
        .map_err(|error| format!("Failed to serialize the knowledge result: {error}"))
}

/// Resolves state's retained storage owner; returns an explicit startup failure when unavailable.
/// 解析 state 保留的存储所有者；不可用时返回明确启动失败。
fn retained_knowledge_runtime(
    state: &SharedServiceBridgeState,
) -> Result<KnowledgeBaseRuntime, String> {
    state
        .lock()
        .map_err(|error| error.to_string())?
        .knowledge_runtime
        .clone()
        .ok_or("ERR_KNOWLEDGE_RUNTIME_UNAVAILABLE".into())
}

/// Computes tool-search visibility for exact `library_ids` from current `runtime` modes; returns storage errors explicitly.
/// 从当前 `runtime` 模式计算精确 `library_ids` 的工具检索可见性；明确返回存储错误。
pub(crate) async fn knowledge_search_enabled(
    runtime: &KnowledgeBaseRuntime,
    library_ids: &[String],
) -> Result<bool, String> {
    // Inspect every selected library so corrupt selections cannot be hidden by an early searchable entry.
    // 检查每个选定库，防止损坏选择被较早可检索条目隐藏。
    let mut enabled = false;
    for id in library_ids {
        // Modes are library-owned and can change independently of session selection.
        // 模式由库拥有，可独立于会话选择变化。
        let library = runtime.get_library(id).await?;
        if library.lifecycle_state != "active" {
            return Err("ERR_KNOWLEDGE_LIBRARY_INACTIVE".into());
        }
        enabled |= library
            .settings
            .ok_or("ERR_KNOWLEDGE_SETTINGS_MISSING")?
            .index_mode
            != KnowledgeIndexMode::Browse;
    }
    Ok(enabled)
}

/// Returns the human lifecycle actor; takes no inputs and never invents AI invocation IDs.
/// 返回人工生命周期执行者；无输入，且绝不虚构 AI 调用身份。
fn human_actor() -> KnowledgeChangeActor {
    KnowledgeChangeActor {
        kind: "user".into(),
        session_id: None,
        turn_id: None,
        tool_call_id: None,
    }
}

/// Lists libraries in state, optionally restricted to project_id; returns lifecycle errors as deletable summaries.
/// 列出 state 中知识库，可选限制至 project_id；以可删除摘要返回生命周期错误。
pub(super) async fn service_list_knowledge_libraries(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .list_libraries(project_id.as_deref())
            .await?,
    )
}

/// Creates an empty library from explicit request and signed catalog facts in state; returns its current summary.
/// 根据显式 request 及 state 中签名目录事实创建空库；返回当前摘要。
pub(super) async fn service_create_knowledge_library(
    state: SharedServiceBridgeState,
    request: KnowledgeLibraryCreateRequest,
) -> Result<Value, String> {
    // Capability admission precedes physical or registry side effects.
    // 能力准入先于物理或注册副作用。
    let settings = knowledge_settings_from_request(&state, request.settings)?;
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .create_library(
                &request.name,
                &request.description,
                &request.project_ids,
                &settings,
            )
            .await?,
    )
}

/// Changes only request's human name/purpose through state; returns the current library.
/// 经 state 仅变更 request 中人工名称及用途；返回当前知识库。
pub(super) async fn service_set_knowledge_library_settings(
    state: SharedServiceBridgeState,
    request: KnowledgeLibrarySettingsUpdateRequest,
) -> Result<Value, String> {
    service_rename_knowledge_library(state, request).await
}

/// Renames request's exact library in state without accepting document/folder content.
/// 重命名 state 中 request 的精确知识库，不接受文档或目录内容。
pub(super) async fn service_rename_knowledge_library(
    state: SharedServiceBridgeState,
    request: KnowledgeLibraryRenameRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .rename_library(&request.library_id, &request.name, &request.description)
            .await?,
    )
}

/// Deletes library_id in state, then removes only its derived profiles; app publishes configuration changes.
/// 删除 state 中 library_id 后仅移除其派生配置档；app 发布配置变更。
pub(super) async fn service_delete_knowledge_library(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    retained_knowledge_runtime(&state)?
        .delete_library(&library_id)
        .await?;
    // Deletion waits for vector readers before their immutable route registrations are removed.
    // 删除等待向量读取者后才移除其不可变路由注册。
    let profiles = crate::public_ai_settings::remove_knowledge_inference_profiles(
        app,
        state.clone(),
        &knowledge_consumer_profile_id(&library_id),
    )
    .await
    .map_err(|error| format!("Library deleted; inference-profile cleanup failed: {error}"))?;
    if profiles.is_empty() {
        return Ok(Value::Null);
    }
    // The current host port owns registrations; no retired service handle is reconstructed.
    // 当前宿主端口拥有注册；不重建已退役服务句柄。
    let port = embedding::knowledge_host_ai_port(&state)?;
    for profile in profiles {
        port.unregister_capability(&embedding::knowledge_embedding_registration_id(&profile))
            .map_err(|error| format!("Library deleted; capability cleanup failed: {error}"))?;
    }
    Ok(Value::Null)
}

/// Changes request's exact mode/route in state; returns a stable resumable operation.
/// 变更 state 中 request 的精确模式及路由；返回稳定可恢复操作。
pub(super) async fn service_set_knowledge_index_mode(
    state: SharedServiceBridgeState,
    request: KnowledgeIndexModeRequestPayload,
) -> Result<Value, String> {
    // Freeze current selected-model capability facts before saving the target mode.
    // 保存目标模式前冻结当前选定模型能力事实。
    let settings = knowledge_settings_from_request(&state, request.settings)?;
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .set_index_mode(&request.library_id, settings)
            .await?,
    )
}

/// Sets or revokes request's project authorization in state; returns no guessed inherited selection.
/// 设置或撤销 state 中 request 的项目授权；不返回推测的继承选择。
pub(super) async fn service_set_knowledge_project_binding(
    state: SharedServiceBridgeState,
    request: KnowledgeProjectBindingRequest,
) -> Result<Value, String> {
    retained_knowledge_runtime(&state)?
        .set_project_binding(
            &request.library_id,
            &request.project_id,
            request.authorized,
            request.default_enabled,
        )
        .await?;
    Ok(Value::Null)
}

/// Lists exact bindings for library_id using state's retained runtime.
/// 使用 state 保留的运行时列出 library_id 的精确绑定。
pub(super) async fn service_list_knowledge_project_bindings(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .list_project_bindings(&library_id)
            .await?,
    )
}

/// Lists library bindings owned by exact project_id in state.
/// 列出 state 中精确 project_id 拥有的库绑定。
pub(super) async fn service_list_project_knowledge_bindings(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .list_bindings_for_project(&project_id)
            .await?,
    )
}

/// Reads request's exact stable operation, or explicitly discovers the latest in state.
/// 读取 state 中 request 的精确稳定操作，或显式发现最新操作。
pub(super) async fn service_get_knowledge_operation(
    state: SharedServiceBridgeState,
    request: KnowledgeOperationRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .get_operation(&request.library_id, request.operation_id.as_deref())
            .await?,
    )
}

/// Cancels request's exact active operation in state; returns whether it transitioned.
/// 取消 state 中 request 的精确活动操作；返回是否发生过渡。
pub(super) async fn service_cancel_knowledge_operation(
    state: SharedServiceBridgeState,
    request: KnowledgeOperationCancelRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .cancel_operation(&request.library_id, &request.operation_id)
            .await?,
    )
}

/// Queues a durable derived-index rebuild for library_id in state; returns the stable job.
/// 为 state 中 library_id 排队持久派生索引重建；返回稳定任务。
pub(super) async fn service_rebuild_knowledge_index(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .rebuild_index_operation(&library_id)
            .await?,
    )
}

/// Repairs library_id's derived indexes using the same explicit resumable rebuild path in state.
/// 使用 state 中相同显式可恢复重建入口修复 library_id 的派生索引。
pub(super) async fn service_repair_knowledge_index(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    service_rebuild_knowledge_index(state, library_id).await
}

/// Reads current derived-index integrity for library_id in state without scheduling work.
/// 读取 state 中 library_id 的当前派生索引完整性，不调度工作。
pub(super) async fn service_get_knowledge_index_integrity(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .index_integrity(&library_id)
            .await?,
    )
}

/// Lists library_id's custom Jieba terms in state; returns exact configuration rows.
/// 列出 state 中 library_id 的自定义结巴词；返回精确配置行。
pub(super) async fn service_list_knowledge_custom_words(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .list_custom_words(&library_id)
            .await?,
    )
}

/// Atomically upserts request's dictionary term under the library lock in state.
/// 在 state 中库锁内原子更新或插入 request 的词典词语。
pub(super) async fn service_upsert_knowledge_custom_word(
    state: SharedServiceBridgeState,
    request: KnowledgeCustomWordRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .upsert_custom_word(&request.library_id, &request.word, request.weight)
            .await?,
    )
}

/// Atomically removes exact word from library_id in state; no content history is edited.
/// 从 state 中 library_id 原子移除精确 word；不编辑内容历史。
pub(super) async fn service_remove_knowledge_custom_word(
    state: SharedServiceBridgeState,
    library_id: String,
    word: String,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .remove_custom_word(&library_id, &word)
            .await?,
    )
}

/// Reads request's bounded logical directory page from state without writable content controls.
/// 从 state 读取 request 的有界逻辑目录页，不提供可写内容控件。
pub(super) async fn service_list_knowledge_library_documents(
    state: SharedServiceBridgeState,
    request: KnowledgeLibraryDocumentListRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .list_content(
                &request.library_id,
                &request.path,
                request.depth,
                request.cursor.as_deref(),
                request.limit,
            )
            .await?,
    )
}

/// Reads only folders from request's directory page; state supplies the same head-bound continuation.
/// 仅读取 request 目录页中的目录；state 提供相同绑定变更头的续读标识。
pub(super) async fn service_list_knowledge_library_folders(
    state: SharedServiceBridgeState,
    request: KnowledgeLibraryDocumentListRequest,
) -> Result<Value, String> {
    // The original page cursor counts scanned entries, preserving progress across document-only pages.
    // 原分页游标计算已扫描条目，使仅文档页面也能保持前进。
    let mut page = retained_knowledge_runtime(&state)?
        .list_content(
            &request.library_id,
            &request.path,
            request.depth,
            request.cursor.as_deref(),
            request.limit,
        )
        .await?;
    page.entries.retain(|entry| entry.kind == "folder");
    knowledge_json(page)
}

/// Reads request's exact document/range in state; returns current metadata and one body window.
/// 读取 state 中 request 的精确文档及范围；返回当前元数据及单份正文窗口。
pub(super) async fn service_read_knowledge_library_document(
    state: SharedServiceBridgeState,
    request: KnowledgeDocumentReadRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .read_document(
                &request.library_id,
                request.document_id.as_deref(),
                request.path.as_deref(),
                request.start_line,
                request.end_line,
            )
            .await?,
    )
}

/// Searches request's single library by its declared mode in state, retaining backlog and partial errors.
/// 按声明模式检索 state 中 request 的单个库，保留积压及部分错误。
pub(super) async fn service_search_knowledge_library(
    state: SharedServiceBridgeState,
    request: KnowledgeLibrarySearchRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .search(&[request.library_id], &[request.query], request.limit)
            .await?,
    )
}

/// Lists request's project-authorized directory through state; returns no library outside its scope.
/// 经 state 列出 request 的项目授权目录；不返回范围外知识库。
pub(super) async fn service_list_knowledge_documents(
    state: SharedServiceBridgeState,
    request: KnowledgeDocumentListRequest,
) -> Result<Value, String> {
    // Revalidate authorization at this service entry, independently of a previous UI selection.
    // 在此服务入口复核授权，独立于先前界面选择。
    let runtime = retained_knowledge_runtime(&state)?;
    require_authorized_library(&runtime, &request.project_id, &request.library_id).await?;
    knowledge_json(
        runtime
            .list_content(
                &request.library_id,
                &request.path,
                request.depth,
                request.cursor.as_deref(),
                request.limit,
            )
            .await?,
    )
}

/// Reads request's project-owned document selector through state with live authorization.
/// 通过 state 实时授权读取 request 的项目所属文档选择器。
pub(super) async fn service_read_knowledge_document(
    state: SharedServiceBridgeState,
    request: KnowledgeProjectDocumentReadRequest,
) -> Result<Value, String> {
    // Authorization belongs to the exact project, not the library's display name.
    // 授权归精确项目所有，不归知识库显示名称所有。
    let runtime = retained_knowledge_runtime(&state)?;
    require_authorized_library(&runtime, &request.project_id, &request.document.library_id).await?;
    service_read_knowledge_library_document(state, request.document).await
}

/// Searches request's exact project-authorized subset in state; every library reports its declared mode.
/// 检索 state 中 request 的精确项目授权子集；每个库报告其声明模式。
pub(super) async fn service_search_knowledge(
    state: SharedServiceBridgeState,
    request: KnowledgeSearchRequest,
) -> Result<Value, String> {
    // Every selector must be authorized; no implicit intersection silently widens or changes intent.
    // 每个选择器均须已授权；不以隐式交集静默扩大或改变意图。
    let runtime = retained_knowledge_runtime(&state)?;
    for library in &request.library_ids {
        require_authorized_library(&runtime, &request.project_id, library).await?;
    }
    knowledge_json(
        runtime
            .search(&request.library_ids, &[request.query], request.limit)
            .await?,
    )
}

/// Reads request's immutable history summaries in state; returns a bounded page.
/// 读取 state 中 request 的不可变历史摘要；返回有界页。
pub(super) async fn service_list_knowledge_changes(
    state: SharedServiceBridgeState,
    request: KnowledgeChangeListRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .list_changes(&request.library_id, request.before_seq, request.limit)
            .await?,
    )
}

/// Reads request's exact change images in state, with item and byte limits.
/// 读取 state 中 request 的精确变更镜像，具有条目及字节限制。
pub(super) async fn service_get_knowledge_change(
    state: SharedServiceBridgeState,
    request: KnowledgeChangeGetRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .get_change_page(
                &request.library_id,
                request.seq,
                request.offset,
                request.limit,
            )
            .await?,
    )
}

/// Reads request's exact historical body side from state for read-only comparison.
/// 从 state 读取 request 的精确历史正文侧别，用于只读比较。
pub(super) async fn service_read_knowledge_change_document(
    state: SharedServiceBridgeState,
    request: KnowledgeChangeDocumentReadRequest,
) -> Result<Value, String> {
    // Only the closed wire enum chooses a stored image column.
    // 仅封闭线协议枚举选择存储镜像列。
    let side = match request.side {
        KnowledgeChangeSideRequest::Before => KnowledgeChangeSide::Before,
        KnowledgeChangeSideRequest::After => KnowledgeChangeSide::After,
    };
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .read_change_document(
                &request.library_id,
                request.seq,
                &request.document_id,
                side,
                request.start_line,
                request.end_line,
            )
            .await?,
    )
}

/// Restores request's human-confirmed point in state as a new reversible content change.
/// 将 state 中 request 的人工确认历史点还原为新可还原内容变更。
pub(super) async fn service_restore_knowledge_library(
    state: SharedServiceBridgeState,
    request: KnowledgeRestoreRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .restore_content(
                &request.library_id,
                request.target_seq,
                human_actor(),
                &request.note,
            )
            .await?,
    )
}

/// Reads request's current desired/active vector progress and bounded failures from state.
/// 从 state 读取 request 的当前期望及活动向量进度与有界失败。
pub(super) async fn service_get_knowledge_embedding_status(
    state: SharedServiceBridgeState,
    request: KnowledgeEmbeddingStatusRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .embedding_status(&request.library_id, request.limit, request.offset)
            .await?,
    )
}

/// Retries request's exhausted current hashes through state; returns a stable operation.
/// 经 state 重试 request 的耗尽当前哈希；返回稳定操作。
pub(super) async fn service_retry_knowledge_embeddings(
    state: SharedServiceBridgeState,
    request: KnowledgeEmbeddingRetryRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .retry_embeddings(&request.library_id, &request.document_ids)
            .await?,
    )
}

/// Exports request's current content/configuration through state to an atomically published package.
/// 经 state 将 request 的当前内容及配置导出至原子发布套件。
pub(super) async fn service_export_knowledge_package(
    state: SharedServiceBridgeState,
    request: KnowledgePackageExportRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .export_package(
                &request.library_id,
                &PathBuf::from(request.destination_directory),
            )
            .await?,
    )
}

/// Inspects request's package bytes through state without creating any library or inference request.
/// 经 state 预检 request 的套件字节，不创建库或推理请求。
pub(super) async fn service_inspect_knowledge_package(
    state: SharedServiceBridgeState,
    request: KnowledgePackageInspectRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .inspect_package(&PathBuf::from(request.archive_path))
            .await?,
    )
}

/// Imports request's exact inspected bytes as a new library in state, with one fresh import change.
/// 在 state 中将 request 的精确预检字节导入为新库，并建立一条全新导入变更。
pub(super) async fn service_import_knowledge_package(
    state: SharedServiceBridgeState,
    request: KnowledgePackageImportRequest,
) -> Result<Value, String> {
    knowledge_json(
        retained_knowledge_runtime(&state)?
            .import_package_as_new(
                &PathBuf::from(request.archive_path),
                &request.name,
                &request.expected_sha256,
            )
            .await?,
    )
}

/// Browses native path through state; returns selectable packages without extracting them.
/// 经 state 浏览原生 path；返回可选套件，不解压套件。
pub(super) fn service_browse_knowledge_packages(
    state: SharedServiceBridgeState,
    path: Option<String>,
) -> Result<Value, String> {
    let listing: KnowledgePackageFileListing = browse_package_files(state, path, "vkb")?;
    serde_json::to_value(listing)
        .map_err(|error| format!("serialize knowledge package file listing: {error}"))
}

/// Requires exact library_id authorization for project_id in runtime; returns an explicit refusal.
/// 要求 runtime 中 project_id 对 library_id 的精确授权；返回明确拒绝。
async fn require_authorized_library(
    runtime: &KnowledgeBaseRuntime,
    project_id: &str,
    library_id: &str,
) -> Result<(), String> {
    let authorized = runtime.authorized_library_ids(project_id, false).await?;
    if authorized.iter().any(|candidate| candidate == library_id) {
        Ok(())
    } else {
        Err(format!(
            "Knowledge library not authorized for the current project: {library_id}"
        ))
    }
}
