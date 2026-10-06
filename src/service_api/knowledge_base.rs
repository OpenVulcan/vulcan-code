// Mirror of `service/knowledge-base-service.ts` (library management side).
// `service/knowledge-base-service.ts` 的镜像（知识库管理部分）。

use serde_json::{Value, json};

use super::{CallKind, Preview, ServiceCall, Slot};

/// Visible document count per page; loaders request one additional sentinel record.
/// 每页可见文档数；加载器额外请求一个哨兵记录。
pub const DOCUMENT_PAGE_SIZE: usize = 20;

// Invalidates all library-owned read projections after content or index mutations.
// 内容或索引变更后，使全部知识库读取投影失效。
const LIBRARY: &[Slot] = &[
    Slot::KnowledgeLibraries,
    Slot::KnowledgeSources,
    Slot::KnowledgeIntegrity,
    Slot::KnowledgeJob,
    Slot::KnowledgeFolders,
    Slot::KnowledgeDocuments,
    Slot::KnowledgeDocument,
    Slot::KnowledgeSearch,
];

/// All libraries (`projectId: null`), as the knowledge management area loads them.
/// 全部知识库（`projectId: null`），与知识库管理区域的加载方式一致。
/// Builds the request for `service_list_knowledge_libraries`.
/// 构建 `service_list_knowledge_libraries` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn list_knowledge_libraries() -> ServiceCall {
    ServiceCall::load(
        "list_knowledge_libraries",
        "service_list_knowledge_libraries",
        json!({ "projectId": null }),
        Slot::KnowledgeLibraries,
    )
}

/// `settings` is a `KnowledgeLibrarySettingsRequest`; `{}` keeps the service defaults.
/// `settings` 为 `KnowledgeLibrarySettingsRequest`；`{}` 保留服务默认值。
/// Builds the request for `service_create_knowledge_library`.
/// 构建 `service_create_knowledge_library` 的请求。
/// `name` is the requested display name or catalog-owned skill name.
/// `name` 为请求的显示名称或目录持有的技能名称。
/// `description` is the complete user-entered description.
/// `description` 为用户输入的完整描述。
/// `project_ids` is the explicit host projects authorized for the new knowledge library.
/// `project_ids` 为明确授权给新知识库的宿主项目。
/// `settings` is the settings document accepted by the matching save API.
/// `settings` 为对应保存 API 接收的设置文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn create_knowledge_library(
    name: &str,
    description: &str,
    project_ids: &[String],
    settings: Value,
) -> ServiceCall {
    ServiceCall::mutate(
        "create_knowledge_library",
        "service_create_knowledge_library",
        json!({ "request": {
            "name": name,
            "description": description,
            "projectIds": project_ids,
            "settings": settings,
        } }),
        &[Slot::KnowledgeLibraries, Slot::KnowledgeBindings],
    )
}

/// `settings` is a partial `KnowledgeLibrarySettingsRequest`.
/// `settings` 为部分 `KnowledgeLibrarySettingsRequest`。
/// Builds the request for `service_set_knowledge_library_settings`.
/// 构建 `service_set_knowledge_library_settings` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// `settings` is the settings document accepted by the matching save API.
/// `settings` 为对应保存 API 接收的设置文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn update_knowledge_library_settings(library_id: &str, settings: Value) -> ServiceCall {
    ServiceCall::mutate(
        "update_knowledge_library_settings",
        "service_set_knowledge_library_settings",
        json!({ "request": { "libraryId": library_id, "settings": settings } }),
        LIBRARY,
    )
}

/// Builds the request for `service_rename_knowledge_library`.
/// 构建 `service_rename_knowledge_library` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// `name` is the requested display name or catalog-owned skill name.
/// `name` 为请求的显示名称或目录持有的技能名称。
/// `description` is the complete user-entered description.
/// `description` 为用户输入的完整描述。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn rename_knowledge_library(library_id: &str, name: &str, description: &str) -> ServiceCall {
    ServiceCall::mutate(
        "rename_knowledge_library",
        "service_rename_knowledge_library",
        json!({ "request": { "libraryId": library_id, "name": name, "description": description } }),
        &[Slot::KnowledgeLibraries],
    )
}

/// Builds the request for `service_delete_knowledge_library`.
/// 构建 `service_delete_knowledge_library` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn delete_knowledge_library(library_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "delete_knowledge_library",
        "service_delete_knowledge_library",
        json!({ "libraryId": library_id }),
        &[Slot::KnowledgeLibraries],
    )
}

/// Builds the request for `service_list_knowledge_sources`.
/// 构建 `service_list_knowledge_sources` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn list_knowledge_sources(library_id: &str) -> ServiceCall {
    ServiceCall::load(
        "list_knowledge_sources",
        "service_list_knowledge_sources",
        json!({ "libraryId": library_id }),
        Slot::KnowledgeSources,
    )
}

/// Loads the open library's authoritative project grants; takes its id and returns a slot loader.
/// 加载当前知识库的权威项目授权；接收其标识并返回槽位加载器。
/// Builds the request for `service_list_knowledge_project_bindings`.
/// 构建 `service_list_knowledge_project_bindings` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn list_knowledge_project_bindings(library_id: &str) -> ServiceCall {
    ServiceCall::load(
        "list_knowledge_project_bindings",
        "service_list_knowledge_project_bindings",
        json!({"libraryId":library_id}),
        Slot::KnowledgeBindings,
    )
}

/// Loads the open library's current import job; takes its id and returns a slot loader.
/// 加载当前知识库的导入任务；接收其标识并返回槽位加载器。
/// Builds the request for `service_get_knowledge_import_job`.
/// 构建 `service_get_knowledge_import_job` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn get_knowledge_import_job(library_id: &str) -> ServiceCall {
    ServiceCall::load(
        "get_knowledge_import_job",
        "service_get_knowledge_import_job",
        json!({"libraryId":library_id}),
        Slot::KnowledgeJob,
    )
}

/// Builds the request for `service_set_knowledge_project_binding`.
/// 构建 `service_set_knowledge_project_binding` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// `project_id` is the exact host project identity.
/// `project_id` 为准确宿主项目身份。
/// `authorized` is the explicit project grant state.
/// `authorized` 为明确的项目授权状态。
/// `default_enabled` is whether the authorized library is enabled by default.
/// `default_enabled` 为已授权知识库是否默认启用。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn set_knowledge_project_binding(
    library_id: &str,
    project_id: &str,
    authorized: bool,
    default_enabled: bool,
) -> ServiceCall {
    ServiceCall::mutate(
        "set_knowledge_project_binding",
        "service_set_knowledge_project_binding",
        json!({ "request": {
            "libraryId": library_id,
            "projectId": project_id,
            "authorized": authorized,
            "defaultEnabled": default_enabled,
        } }),
        &[Slot::KnowledgeLibraries, Slot::KnowledgeBindings],
    )
}

/// Scans a directory before importing; `source_id` re-scans an existing source.
/// 导入前扫描目录；`source_id` 表示重新扫描已有来源。
/// Builds the request for `service_preview_knowledge_import`.
/// 构建 `service_preview_knowledge_import` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// `source_root` is the selected directory root inspected by the service.
/// `source_root` 为服务检查的所选目录根。
/// `source_id` is the existing source ID for a rescan; None creates a source.
/// `source_id` 为重新扫描时的已有来源标识；None 创建来源。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn preview_knowledge_import(
    library_id: &str,
    source_root: &str,
    source_id: Option<&str>,
) -> ServiceCall {
    ServiceCall {
        function: "preview_knowledge_import",
        method: "service_preview_knowledge_import",
        params: json!({ "libraryId": library_id, "sourceRoot": source_root, "sourceId": source_id }),
        kind: CallKind::Preview(Preview::KnowledgeImport),
    }
}

/// `importKnowledgeDirectory` with the preview fingerprint and the ready files.
/// 以预检指纹与就绪文件调用 `importKnowledgeDirectory`。
/// Builds the request for `service_import_knowledge_directory`.
/// 构建 `service_import_knowledge_directory` 的请求。
/// `preview` is the read-only first-step call or its declared preview selector.
/// `preview` 为只读第一步调用或其声明的预检选择器。
/// `result` is the exact service response for the preceding preview.
/// `result` 为前序预检的准确服务响应。
/// Returns an unexecuted call; None rejects incomplete or invalid preview evidence.
/// 返回尚未执行的调用；None 拒绝缺失或无效的预检证据。
pub fn import_knowledge_directory(preview: &ServiceCall, result: &Value) -> Option<ServiceCall> {
    // Fingerprint.
    // 服务签发的内容或预检指纹。
    let fingerprint = result.get("fingerprint")?.as_str()?;
    if fingerprint.is_empty() {
        return None;
    }
    // Selected.
    // 当前选择。
    let mut selected = Vec::new();
    for entry in result.get("entries")?.as_array()? {
        if entry.get("disposition")?.as_str()? == "ready" {
            // Key.
            // 当前结构持有的键。
            let key = entry.get("key")?.as_str()?;
            if key.is_empty() || selected.iter().any(|existing| existing == key) {
                return None;
            }
            selected.push(key.to_string());
        }
    }
    Some(ServiceCall::mutate(
        "import_knowledge_directory",
        "service_import_knowledge_directory",
        json!({"request":{
            "libraryId":preview.params.get("libraryId")?.as_str()?,
            "sourceRoot":preview.params.get("sourceRoot")?.as_str()?,
            "sourceId":preview.params.get("sourceId")?,
            "previewFingerprint":fingerprint, "selectedFileKeys":selected
        }}),
        LIBRARY,
    ))
}

/// Builds the request for `service_cancel_knowledge_import`.
/// 构建 `service_cancel_knowledge_import` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// `job_id` is the exact job identity; None reads the latest job where allowed.
/// `job_id` 为准确任务身份；协议允许时 None 读取最近任务。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn cancel_knowledge_import(library_id: &str, job_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "cancel_knowledge_import",
        "service_cancel_knowledge_import",
        json!({ "libraryId": library_id, "jobId": job_id }),
        LIBRARY,
    )
}

/// Builds the request for `service_rebuild_knowledge_index`.
/// 构建 `service_rebuild_knowledge_index` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn rebuild_knowledge_index(library_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "rebuild_knowledge_index",
        "service_rebuild_knowledge_index",
        json!({ "libraryId": library_id }),
        LIBRARY,
    )
}

/// Builds the request for `service_repair_knowledge_index`.
/// 构建 `service_repair_knowledge_index` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn repair_knowledge_index(library_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "repair_knowledge_index",
        "service_repair_knowledge_index",
        json!({ "libraryId": library_id }),
        LIBRARY,
    )
}

/// Builds the request for `service_get_knowledge_index_integrity`.
/// 构建 `service_get_knowledge_index_integrity` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn get_knowledge_index_integrity(library_id: &str) -> ServiceCall {
    ServiceCall::load(
        "get_knowledge_index_integrity",
        "service_get_knowledge_index_integrity",
        json!({ "libraryId": library_id }),
        Slot::KnowledgeIntegrity,
    )
}

/// `retrieval_mode` is `raw_bm25`, `display_bm25` or `display_hybrid`; `None` uses the library's.
/// `retrieval_mode` 为 `raw_bm25`、`display_bm25` 或 `display_hybrid`；`None` 使用知识库设置。
/// Builds the request for `service_search_knowledge_library`.
/// 构建 `service_search_knowledge_library` 的请求。
/// `library_id` is the library ID returned by the managed knowledge catalog.
/// `library_id` 为受管理知识库目录返回的知识库标识。
/// `query` is the requested search text.
/// `query` 为请求的搜索文本。
/// `retrieval_mode` is raw_bm25, display_bm25 or display_hybrid; None follows the library.
/// `retrieval_mode` 为raw_bm25、display_bm25 或 display_hybrid；None 跟随知识库。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn search_managed_knowledge_library(
    library_id: &str,
    query: &str,
    retrieval_mode: Option<&str>,
) -> ServiceCall {
    ServiceCall::load(
        "search_managed_knowledge_library",
        "service_search_knowledge_library",
        json!({ "request": {
            "libraryId": library_id,
            "query": query,
            "limit": 20,
            "retrievalMode": retrieval_mode,
        } }),
        Slot::KnowledgeSearch,
    )
}
/// Requests exact document summaries using the managed-library browse contract.
/// 使用托管知识库浏览契约请求准确文档摘要。
/// `library_id` owns documents; `folder_path` None includes all folders; `limit` and `offset` paginate.
/// `library_id` 持有文档；`folder_path` 为 None 时包含全部目录；`limit` 与 `offset` 用于分页。
/// Returns an unexecuted document-list loader.
/// 返回尚未执行的文档列表加载器。
pub fn list_managed_knowledge_documents(
    library_id: &str,
    folder_path: Option<&str>,
    limit: usize,
    offset: usize,
) -> ServiceCall {
    ServiceCall::load(
        "list_managed_knowledge_documents",
        "service_list_knowledge_library_documents",
        json!({"request":{"libraryId":library_id,"folderPath":folder_path,"limit":limit,"offset":offset}}),
        Slot::KnowledgeDocuments,
    )
}

/// Lists managed relative folders for one library without examining the local filesystem.
/// 列出单个知识库的托管相对目录，不检查本地文件系统。
/// `library_id` is its catalog identity; returns an unexecuted folder-list loader.
/// `library_id` 为其目录身份；返回尚未执行的目录列表加载器。
pub fn list_managed_knowledge_document_folders(library_id: &str) -> ServiceCall {
    ServiceCall::load(
        "list_managed_knowledge_document_folders",
        "service_list_knowledge_library_folders",
        json!({"libraryId":library_id}),
        Slot::KnowledgeFolders,
    )
}

/// Reads the authoritative Markdown content of a selected document and corpus.
/// 读取所选文档及语料的权威 Markdown 正文。
/// `library_id` owns `document_id`; `corpus` is the declared raw or display variant.
/// `library_id` 持有 `document_id`；`corpus` 为已声明的 raw 或 display 变体。
/// Returns an unexecuted reader loader; missing display content remains a service error.
/// 返回尚未执行的阅读加载器；缺失展示正文时保留服务错误。
pub fn read_managed_knowledge_document(
    library_id: &str,
    document_id: &str,
    corpus: &str,
) -> ServiceCall {
    ServiceCall::load(
        "read_managed_knowledge_document",
        "service_read_knowledge_library_document",
        json!({"libraryId":library_id,"documentId":document_id,"corpus":corpus}),
        Slot::KnowledgeDocument,
    )
}

/// Previews the display-corpus build before selecting the server-approved files.
/// 在选择服务批准文件前预检展示语料构建。
/// `library_id` is the catalog identity; returns an unexecuted preview call.
/// `library_id` 为目录身份；返回尚未执行的预检调用。
pub fn preview_knowledge_display(library_id: &str) -> ServiceCall {
    ServiceCall {
        function: "preview_knowledge_display",
        method: "service_preview_knowledge_display",
        params: json!({"libraryId":library_id}),
        kind: CallKind::Preview(Preview::KnowledgeDisplay),
    }
}

/// Freezes the display preflight fingerprint and distinct ready-file whitelist.
/// 冻结展示预检指纹及互不重复的就绪文件允许列表。
/// `preview` identifies the library; `result` is its exact KnowledgeFilePreview response.
/// `preview` 标识知识库；`result` 为其准确 KnowledgeFilePreview 响应。
/// Returns the confirmed build call, or None for incomplete server evidence.
/// 返回已确认构建调用；服务证据不完整时返回 None。
pub fn build_knowledge_display_index(preview: &ServiceCall, result: &Value) -> Option<ServiceCall> {
    // Empty fingerprints cannot authorize a build.
    // 空指纹不能授权构建。
    let fingerprint = result.get("fingerprint")?.as_str()?;
    if fingerprint.is_empty() {
        return None;
    }
    // Preserve exact file identities and reject duplicates rather than silently widening selection.
    // 保留准确文件身份并拒绝重复项，避免静默扩大选择。
    let mut selected = Vec::new();
    for entry in result.get("entries")?.as_array()? {
        if entry.get("disposition")?.as_str()? == "ready" {
            // Server-owned relative file key.
            // 服务持有的相对文件键。
            let key = entry.get("key")?.as_str()?;
            if key.is_empty() || selected.iter().any(|existing| existing == key) {
                return None;
            }
            selected.push(key.to_string());
        }
    }
    Some(ServiceCall::mutate(
        "build_knowledge_display_index",
        "service_build_knowledge_display_index",
        json!({"libraryId":preview.params.get("libraryId")?.as_str()?,
               "confirmation":{"previewFingerprint":fingerprint,"selectedFileKeys":selected}}),
        LIBRARY,
    ))
}

/// Builds the vector index for an enabled embedding configuration.
/// 为已启用的向量配置构建向量索引。
/// `library_id` is the selected catalog identity; returns an unexecuted job-start call.
/// `library_id` 为所选目录身份；返回尚未执行的任务启动调用。
pub fn build_knowledge_vector_index(library_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "build_knowledge_vector_index",
        "service_build_knowledge_vector_index",
        json!({"libraryId":library_id}),
        LIBRARY,
    )
}
