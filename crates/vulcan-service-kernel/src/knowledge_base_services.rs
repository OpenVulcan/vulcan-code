//! Host-native knowledge-base service handlers.
//! 宿主原生知识库服务处理器。

use super::*;
use std::{fs, path::PathBuf};
use vulcan_service_api::{
    KnowledgeCustomWordRequest, KnowledgeDocumentListRequest, KnowledgeImportRequest,
    KnowledgeLibraryCreateRequest, KnowledgeLibraryDocumentListRequest,
    KnowledgeLibraryRenameRequest, KnowledgeLibrarySearchRequest, KnowledgePackageExportRequest,
    KnowledgePackageFileListing, KnowledgePackageFileSummary, KnowledgePackageImportRequest,
    KnowledgePackageImportStrategy, KnowledgePackageInspectRequest, KnowledgeProjectBindingRequest,
    KnowledgeSearchRequest, SessionKnowledgeLibrarySelectionRequest,
    SessionKnowledgeLibrarySelectionSummary,
};
use vulcan_workbench_model::{
    SessionKnowledgeLibrarySelection, SessionKnowledgeLibrarySelectionMode,
};
use vulcan_workspace_store::knowledge_base::KnowledgeBaseRuntime;

/// Returns the retained knowledge runtime without holding the service-state lock across I/O.
/// 返回保留的知识库运行时，且不会在 I/O 期间持有服务状态锁。
fn retained_knowledge_runtime(
    state: &SharedServiceBridgeState,
) -> Result<KnowledgeBaseRuntime, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .knowledge_runtime
        .clone()
        .ok_or_else(|| String::from("service is still starting; knowledge runtime is not ready"))
}

/// Lists libraries, optionally constrained to one project authorization set.
/// 列出知识库，并可选约束到一个项目授权集合。
pub(super) async fn service_list_knowledge_libraries(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
) -> Result<Value, String> {
    let runtime = retained_knowledge_runtime(&state)?;
    let libraries = runtime.list_libraries(project_id.as_deref()).await?;
    serde_json::to_value(libraries)
        .map_err(|error| format!("serialize knowledge libraries: {error}"))
}

/// Creates one knowledge library and returns its registry summary.
/// 创建一个知识库并返回其注册摘要。
pub(super) async fn service_create_knowledge_library(
    state: SharedServiceBridgeState,
    request: KnowledgeLibraryCreateRequest,
) -> Result<Value, String> {
    let runtime = retained_knowledge_runtime(&state)?;
    let library = runtime
        .create_library(&request.name, &request.description, &request.project_ids)
        .await?;
    serde_json::to_value(library).map_err(|error| format!("serialize knowledge library: {error}"))
}

/// Renames one library without changing its identity.
/// 在不改变身份的前提下重命名一个知识库。
pub(super) async fn service_rename_knowledge_library(
    state: SharedServiceBridgeState,
    request: KnowledgeLibraryRenameRequest,
) -> Result<Value, String> {
    let runtime = retained_knowledge_runtime(&state)?;
    let library = runtime
        .rename_library(&request.library_id, &request.name, &request.description)
        .await?;
    serde_json::to_value(library).map_err(|error| format!("serialize knowledge library: {error}"))
}

/// Deletes one physically isolated knowledge library.
/// 删除一个物理隔离知识库。
pub(super) async fn service_delete_knowledge_library(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    retained_knowledge_runtime(&state)?
        .delete_library(&library_id)
        .await?;
    Ok(json!({ "deleted": true, "libraryId": library_id }))
}

/// Updates one project authorization binding.
/// 更新一条项目授权绑定。
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
    Ok(json!({ "saved": true }))
}

/// Lists project bindings for one library.
/// 列出一个知识库的项目绑定。
pub(super) async fn service_list_knowledge_project_bindings(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    let bindings = retained_knowledge_runtime(&state)?
        .list_project_bindings(&library_id)
        .await?;
    serde_json::to_value(bindings).map_err(|error| format!("serialize knowledge bindings: {error}"))
}

/// Lists every active-library binding for one project in a single service request.
/// 通过单次服务请求列出一个项目的全部活动知识库绑定。
pub(super) async fn service_list_project_knowledge_bindings(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<Value, String> {
    let bindings = retained_knowledge_runtime(&state)?
        .list_bindings_for_project(&project_id)
        .await?;
    serde_json::to_value(bindings)
        .map_err(|error| format!("serialize project knowledge bindings: {error}"))
}

/// Lists every durable import source attached to one library.
/// 列出关联到一个知识库的全部持久化导入来源。
///
/// Parameters: `state` owns the retained runtime and `library_id` selects the physical library.
/// Returns: a browser-facing JSON array of source summaries.
/// 参数：`state` 持有保留运行时，`library_id` 选择物理知识库。返回值：面向浏览器的来源摘要 JSON 数组。
pub(super) async fn service_list_knowledge_sources(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    let sources = retained_knowledge_runtime(&state)?
        .list_sources(&library_id)
        .await?;
    serde_json::to_value(sources).map_err(|error| format!("serialize knowledge sources: {error}"))
}

/// Starts one detached recursive Markdown import with durable progress state.
/// 使用持久化进度状态启动一个分离式递归 Markdown 导入。
pub(super) async fn service_import_knowledge_directory(
    state: SharedServiceBridgeState,
    request: KnowledgeImportRequest,
) -> Result<Value, String> {
    let job = retained_knowledge_runtime(&state)?
        .start_import_markdown_directory_for_source(
            &request.library_id,
            request.source_id.as_deref(),
            &PathBuf::from(request.source_root),
        )
        .await?;
    serde_json::to_value(job).map_err(|error| format!("serialize knowledge operation: {error}"))
}

/// Requests cooperative cancellation for one exact active knowledge operation.
/// 请求协作取消一个精确的活动知识库操作。
pub(super) async fn service_cancel_knowledge_import(
    state: SharedServiceBridgeState,
    library_id: String,
    job_id: String,
) -> Result<Value, String> {
    let cancel_requested = retained_knowledge_runtime(&state)?
        .cancel_operation(&library_id, &job_id)
        .await?;
    Ok(json!({
        "cancelRequested": cancel_requested,
        "libraryId": library_id,
        "jobId": job_id
    }))
}

/// Returns one library's latest dedicated operation progress row.
/// 返回一个知识库最近的专用操作进度记录。
pub(super) async fn service_get_knowledge_import_job(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    let job = retained_knowledge_runtime(&state)?
        .latest_import_job(&library_id)
        .await?;
    serde_json::to_value(job).map_err(|error| format!("serialize knowledge import job: {error}"))
}

/// Starts rebuilding one library's Jieba FTS index from authoritative rows.
/// 启动从权威记录重建一个知识库的 Jieba FTS 索引。
pub(super) async fn service_rebuild_knowledge_index(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    let job = retained_knowledge_runtime(&state)?
        .start_rebuild_index(&library_id)
        .await?;
    serde_json::to_value(job).map_err(|error| format!("serialize knowledge rebuild: {error}"))
}

/// Starts repairing missing or mismatched Jieba FTS rows without discarding healthy rows.
/// 启动补全缺失或不匹配的 Jieba FTS 记录，同时保留健康记录。
pub(super) async fn service_repair_knowledge_index(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    let job = retained_knowledge_runtime(&state)?
        .start_repair_index(&library_id)
        .await?;
    serde_json::to_value(job).map_err(|error| format!("serialize knowledge repair: {error}"))
}

/// Returns one library's complete metadata and physical FTS integrity snapshot.
/// 返回一个知识库的完整元数据与物理 FTS 完整性快照。
pub(super) async fn service_get_knowledge_index_integrity(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    let integrity = retained_knowledge_runtime(&state)?
        .inspect_index_integrity(&library_id)
        .await?;
    serde_json::to_value(integrity)
        .map_err(|error| format!("serialize knowledge index integrity: {error}"))
}

/// Lists library-local Jieba custom dictionary entries.
/// 列出知识库局部 Jieba 自定义词典条目。
pub(super) async fn service_list_knowledge_custom_words(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    let words = retained_knowledge_runtime(&state)?
        .list_custom_words(&library_id)
        .await?;
    serde_json::to_value(words)
        .map_err(|error| format!("serialize knowledge custom words: {error}"))
}

/// Upserts one library-local Jieba custom dictionary entry.
/// 写入一个知识库局部 Jieba 自定义词典条目。
pub(super) async fn service_upsert_knowledge_custom_word(
    state: SharedServiceBridgeState,
    request: KnowledgeCustomWordRequest,
) -> Result<Value, String> {
    retained_knowledge_runtime(&state)?
        .upsert_custom_word(&request.library_id, &request.word, request.weight)
        .await?;
    Ok(json!({ "saved": true }))
}

/// Removes one library-local Jieba custom dictionary entry.
/// 删除一个知识库局部 Jieba 自定义词典条目。
pub(super) async fn service_remove_knowledge_custom_word(
    state: SharedServiceBridgeState,
    library_id: String,
    word: String,
) -> Result<Value, String> {
    retained_knowledge_runtime(&state)?
        .remove_custom_word(&library_id, &word)
        .await?;
    Ok(json!({ "removed": true }))
}

/// Lists documents directly from one library for the trusted management surface.
/// 为受信任的管理界面直接列出一个知识库中的文档。
pub(super) async fn service_list_knowledge_library_documents(
    state: SharedServiceBridgeState,
    request: KnowledgeLibraryDocumentListRequest,
) -> Result<Value, String> {
    let documents = retained_knowledge_runtime(&state)?
        .list_documents(
            &request.library_id,
            request.folder_path.as_deref(),
            request.limit,
            request.offset,
        )
        .await?;
    serde_json::to_value(documents)
        .map_err(|error| format!("serialize managed knowledge documents: {error}"))
}

/// Lists distinct document folders for lazy management-tree navigation.
/// 为管理界面的延迟目录树导航列出唯一文档目录。
pub(super) async fn service_list_knowledge_library_folders(
    state: SharedServiceBridgeState,
    library_id: String,
) -> Result<Value, String> {
    let folders = retained_knowledge_runtime(&state)?
        .list_document_folders(&library_id)
        .await?;
    serde_json::to_value(folders)
        .map_err(|error| format!("serialize managed knowledge folders: {error}"))
}

/// Reads one authoritative Markdown document for the trusted management surface.
/// 为受信任的管理界面读取一份权威 Markdown 文档。
pub(super) async fn service_read_knowledge_library_document(
    state: SharedServiceBridgeState,
    library_id: String,
    document_id: String,
) -> Result<Value, String> {
    let document = retained_knowledge_runtime(&state)?
        .read_document(&library_id, &document_id)
        .await?;
    serde_json::to_value(document)
        .map_err(|error| format!("serialize managed knowledge document: {error}"))
}

/// Searches one selected library directly for the trusted management surface.
/// 为受信任的管理界面直接检索一个选中的知识库。
pub(super) async fn service_search_knowledge_library(
    state: SharedServiceBridgeState,
    request: KnowledgeLibrarySearchRequest,
) -> Result<Value, String> {
    let runtime = retained_knowledge_runtime(&state)?;
    let result = runtime
        .search(
            std::slice::from_ref(&request.library_id),
            &request.query,
            request.limit,
        )
        .await?;
    serde_json::to_value(result)
        .map_err(|error| format!("serialize managed knowledge search: {error}"))
}

/// Exports one complete ready library into a portable .vkb package.
/// 将一个完整且就绪的知识库导出为可移植 .vkb 包。
pub(super) async fn service_export_knowledge_package(
    state: SharedServiceBridgeState,
    request: KnowledgePackageExportRequest,
) -> Result<Value, String> {
    let result = retained_knowledge_runtime(&state)?
        .export_package(
            &request.library_id,
            &PathBuf::from(request.destination_directory),
        )
        .await?;
    serde_json::to_value(result)
        .map_err(|error| format!("serialize knowledge package export: {error}"))
}

/// Inspects one .vkb package without mutating library state.
/// 检查一个 .vkb 包且不改变知识库状态。
pub(super) async fn service_inspect_knowledge_package(
    state: SharedServiceBridgeState,
    request: KnowledgePackageInspectRequest,
) -> Result<Value, String> {
    let result = retained_knowledge_runtime(&state)?
        .inspect_package(&PathBuf::from(request.archive_path))
        .await?;
    serde_json::to_value(result)
        .map_err(|error| format!("serialize knowledge package inspection: {error}"))
}

/// Imports one inspected package using one explicit non-overwriting or replacement strategy.
/// 使用一个显式的不覆盖或替换策略导入已检查的知识库包。
pub(super) async fn service_import_knowledge_package(
    state: SharedServiceBridgeState,
    request: KnowledgePackageImportRequest,
) -> Result<Value, String> {
    let runtime = retained_knowledge_runtime(&state)?;
    let archive_path = PathBuf::from(request.archive_path);
    let result = match request.strategy {
        KnowledgePackageImportStrategy::Create => {
            if request.target_library_id.is_some() {
                return Err(String::from("新建导入不得指定覆盖目标。"));
            }
            runtime
                .import_package_as_new(&archive_path, &request.name)
                .await?
        }
        KnowledgePackageImportStrategy::Replace => {
            let target_library_id = request
                .target_library_id
                .ok_or_else(|| String::from("覆盖导入必须指定精确目标知识库。"))?;
            runtime
                .replace_library_from_package(&archive_path, &target_library_id, &request.name)
                .await?
        }
    };
    serde_json::to_value(result)
        .map_err(|error| format!("serialize knowledge package import: {error}"))
}

/// Browses directories and ordinary .vkb files for the package-import picker.
/// 为知识库包导入选择器浏览目录和普通 .vkb 文件。
pub(super) fn service_browse_knowledge_packages(
    state: SharedServiceBridgeState,
    path: Option<String>,
) -> Result<Value, String> {
    let directory = service_browse_project_directories(state, path)?;
    let mut packages = fs::read_dir(PathBuf::from(&directory.path))
        .map_err(|error| format!("读取知识库包目录失败：{error}"))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let metadata = fs::symlink_metadata(entry.path()).ok()?;
            if !metadata.is_file()
                || entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_none_or(|extension| !extension.eq_ignore_ascii_case("vkb"))
            {
                return None;
            }
            Some(KnowledgePackageFileSummary {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: normalized_workspace_path_text_from_path(&entry.path()),
                byte_size: metadata.len(),
            })
        })
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    serde_json::to_value(KnowledgePackageFileListing {
        directory,
        packages,
    })
    .map_err(|error| format!("serialize knowledge package file listing: {error}"))
}

/// Lists a bounded document page after project authorization validation.
/// 在项目授权校验后列出一个有界文档分页。
pub(super) async fn service_list_knowledge_documents(
    state: SharedServiceBridgeState,
    request: KnowledgeDocumentListRequest,
) -> Result<Value, String> {
    let runtime = retained_knowledge_runtime(&state)?;
    require_authorized_library(&runtime, &request.project_id, &request.library_id).await?;
    let documents = runtime
        .list_documents(
            &request.library_id,
            request.folder_path.as_deref(),
            request.limit,
            request.offset,
        )
        .await?;
    serde_json::to_value(documents)
        .map_err(|error| format!("serialize knowledge documents: {error}"))
}

/// Reads one authoritative document after project authorization validation.
/// 在项目授权校验后读取一份权威文档。
pub(super) async fn service_read_knowledge_document(
    state: SharedServiceBridgeState,
    project_id: String,
    library_id: String,
    document_id: String,
) -> Result<Value, String> {
    let runtime = retained_knowledge_runtime(&state)?;
    require_authorized_library(&runtime, &project_id, &library_id).await?;
    let document = runtime.read_document(&library_id, &document_id).await?;
    serde_json::to_value(document).map_err(|error| format!("serialize knowledge document: {error}"))
}

/// Executes a project-authorized BM25 search.
/// 执行一次通过项目授权校验的 BM25 检索。
pub(super) async fn service_search_knowledge(
    state: SharedServiceBridgeState,
    request: KnowledgeSearchRequest,
) -> Result<Value, String> {
    let runtime = retained_knowledge_runtime(&state)?;
    let authorized: BTreeSet<String> = runtime
        .authorized_library_ids(&request.project_id, false)
        .await?
        .into_iter()
        .collect();
    if let Some(denied) = request
        .library_ids
        .iter()
        .find(|library_id| !authorized.contains(*library_id))
    {
        return Err(format!("知识库未授权给当前项目：{denied}"));
    }
    let result = runtime
        .search(&request.library_ids, &request.query, request.limit)
        .await?;
    serde_json::to_value(result).map_err(|error| format!("serialize knowledge search: {error}"))
}

/// Persists a project-authorized session knowledge selection as an addon namespace.
/// 将通过项目授权校验的会话知识库选择持久化为附属命名空间。
pub(super) async fn service_set_session_knowledge_libraries(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionKnowledgeLibrarySelectionRequest,
) -> Result<Value, String> {
    let session_id = parse_session_id(&request.conversation_id)
        .ok_or_else(|| String::from("会话标识符无效，无法保存知识库选择。"))?;
    let mode = match request.mode.as_str() {
        "inherit" => SessionKnowledgeLibrarySelectionMode::Inherit,
        "explicit" => SessionKnowledgeLibrarySelectionMode::Explicit,
        value => return Err(format!("未知知识库选择模式：{value}")),
    };
    let runtime = retained_knowledge_runtime(&state)?;
    let authorized: BTreeSet<String> = runtime
        .authorized_library_ids(&request.project_id, false)
        .await?
        .into_iter()
        .collect();
    if let Some(denied) = request
        .library_ids
        .iter()
        .find(|library_id| !authorized.contains(*library_id))
    {
        return Err(format!("知识库未授权给当前项目：{denied}"));
    }

    let (controller_runtime, workspace_snapshot) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let workspace_snapshot = guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if owning_project_id_for_session(&workspace_snapshot, session_id).as_deref()
            != Some(request.project_id.as_str())
        {
            return Err(String::from(
                "当前会话与请求项目不匹配，已拒绝保存知识库选择。",
            ));
        }
        (controller_runtime, workspace_snapshot)
    };
    let selection = SessionKnowledgeLibrarySelection {
        mode,
        library_ids: request.library_ids,
    }
    .normalized();
    let updated_at = current_unix_millis_text();
    let mut addon_state = SessionAddonState::new(session_id);
    let _ = addon_state.set_knowledge_library_selection(&selection, updated_at);
    invalidate_webui_session_addon_persist_version(
        &state,
        session_id,
        SessionAddonNamespace::KnowledgeBases,
    )?;
    let persist_lock = webui_workspace_persist_lock();
    let _persist_guard = persist_lock.lock().await;
    let persisted = persist_workspace_session_addon_state_update(
        controller_runtime,
        workspace_snapshot,
        session_id,
        addon_state,
    )
    .await?;
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("service workspace snapshot disappeared"))?;
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(SessionRuntimeStateEntry::default);
        runtime_state.knowledge_library_selection = selection;
    }
    emit_session_addon_changed_event(
        &app,
        session_id,
        &[SessionAddonNamespace::KnowledgeBases],
        persisted.revision,
        latest_session_addon_updated_at(&persisted),
    );
    serde_json::to_value(persisted)
        .map_err(|error| format!("serialize session knowledge selection: {error}"))
}

/// Returns one session's persisted and effective project-authorized knowledge selection.
/// 返回一个会话持久化且经过项目授权后的有效知识库选择。
pub(super) async fn service_get_session_knowledge_libraries(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<Value, String> {
    let session_id = parse_session_id(&conversation_id)
        .ok_or_else(|| String::from("会话标识符无效，无法读取知识库选择。"))?;
    let (runtime, project_id, selection) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime = guard.knowledge_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; knowledge runtime is not ready")
        })?;
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let project_id = owning_project_id_for_session(workspace, session_id)
            .ok_or_else(|| String::from("当前会话不属于可用项目。"))?;
        let selection = workspace
            .session_runtime_state
            .get(&session_id)
            .map(|runtime_state| runtime_state.knowledge_library_selection.clone())
            .unwrap_or_default();
        (runtime, project_id, selection)
    };
    let effective_library_ids = match selection.mode {
        SessionKnowledgeLibrarySelectionMode::Inherit => {
            runtime.authorized_library_ids(&project_id, true).await?
        }
        SessionKnowledgeLibrarySelectionMode::Explicit => {
            let authorized: BTreeSet<String> = runtime
                .authorized_library_ids(&project_id, false)
                .await?
                .into_iter()
                .collect();
            selection
                .library_ids
                .iter()
                .filter(|library_id| authorized.contains(*library_id))
                .cloned()
                .collect()
        }
    };
    let summary = SessionKnowledgeLibrarySelectionSummary {
        mode: match selection.mode {
            SessionKnowledgeLibrarySelectionMode::Inherit => String::from("inherit"),
            SessionKnowledgeLibrarySelectionMode::Explicit => String::from("explicit"),
        },
        library_ids: selection.library_ids,
        effective_library_ids,
    };
    serde_json::to_value(summary)
        .map_err(|error| format!("serialize session knowledge selection: {error}"))
}

/// Rejects access unless the exact library is authorized for the exact project.
/// 除非精确知识库已授权给精确项目，否则拒绝访问。
async fn require_authorized_library(
    runtime: &KnowledgeBaseRuntime,
    project_id: &str,
    library_id: &str,
) -> Result<(), String> {
    let authorized = runtime.authorized_library_ids(project_id, false).await?;
    if authorized.iter().any(|candidate| candidate == library_id) {
        Ok(())
    } else {
        Err(format!("知识库未授权给当前项目：{library_id}"))
    }
}
