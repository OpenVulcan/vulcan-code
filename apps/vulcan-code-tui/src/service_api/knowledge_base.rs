// Exact mirror of the current WebUI knowledge service functions and request envelopes.
// 当前 WebUI 知识库服务函数及请求封装的精确镜像。

use super::{CallKind, Preview, ServiceCall, Slot};
use serde_json::json;
use vulcan_service_api::*;

// All library-owned projections refresh through their last exact loader after a committed mutation.
// 全部库所属投影在变更提交后经其最后精确加载器刷新。
const LIBRARY: &[Slot] = &[
    Slot::KnowledgeLibraries,
    Slot::KnowledgeBindings,
    Slot::KnowledgeIntegrity,
    Slot::KnowledgeJob,
    Slot::KnowledgeEmbeddings,
    Slot::KnowledgeWords,
    Slot::KnowledgeFolders,
    Slot::KnowledgeDocuments,
    Slot::KnowledgeDocument,
    Slot::KnowledgeChanges,
    Slot::KnowledgeChange,
];

/// Creates an empty library using the explicit mode and admitted model route.
/// 以显式模式及准入模型路由创建空库。
/// Parameters: request is the exact shared KnowledgeLibraryCreateRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeLibraryCreateRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn create_knowledge_library(request: KnowledgeLibraryCreateRequest) -> ServiceCall {
    ServiceCall::mutate(
        "create_knowledge_library",
        "service_create_knowledge_library",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Updates only the human library name and purpose.
/// 仅更新人工知识库名称及用途。
/// Parameters: request is the exact shared KnowledgeLibrarySettingsUpdateRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeLibrarySettingsUpdateRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
#[allow(
    dead_code,
    reason = "Complete one-to-one WebUI request mirror; this client uses renameKnowledgeLibrary for metadata."
)]
pub fn update_knowledge_library_settings(
    request: KnowledgeLibrarySettingsUpdateRequest,
) -> ServiceCall {
    ServiceCall::mutate(
        "update_knowledge_library_settings",
        "service_set_knowledge_library_settings",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Saves the exact library name and purpose.
/// 保存精确知识库名称及用途。
/// Parameters: request is the exact shared KnowledgeLibraryRenameRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeLibraryRenameRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn rename_knowledge_library(request: KnowledgeLibraryRenameRequest) -> ServiceCall {
    ServiceCall::mutate(
        "rename_knowledge_library",
        "service_rename_knowledge_library",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Persists the explicit target mode and returns its durable operation.
/// 持久化显式目标模式并返回其持久操作。
/// Parameters: request is the exact shared KnowledgeIndexModeRequestPayload; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeIndexModeRequestPayload；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn set_knowledge_index_mode(request: KnowledgeIndexModeRequestPayload) -> ServiceCall {
    ServiceCall::mutate(
        "set_knowledge_index_mode",
        "service_set_knowledge_index_mode",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Changes one project's authorization and inherited enablement.
/// 变更一个项目的授权及继承启用。
/// Parameters: request is the exact shared KnowledgeProjectBindingRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeProjectBindingRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn set_knowledge_project_binding(request: KnowledgeProjectBindingRequest) -> ServiceCall {
    ServiceCall::mutate(
        "set_knowledge_project_binding",
        "service_set_knowledge_project_binding",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Reads the exact stable operation or explicitly discovers the latest.
/// 读取精确稳定操作，或显式发现最新操作。
/// Parameters: request is the exact shared KnowledgeOperationRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeOperationRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn get_knowledge_operation(request: KnowledgeOperationRequest) -> ServiceCall {
    ServiceCall::load(
        "get_knowledge_operation",
        "service_get_knowledge_operation",
        json!({"request":request}),
        Slot::KnowledgeJob,
    )
}

/// Cancels one exact operation and returns whether it transitioned.
/// 取消一个精确操作并返回是否发生过渡。
/// Parameters: request is the exact shared KnowledgeOperationCancelRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeOperationCancelRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn cancel_knowledge_operation(request: KnowledgeOperationCancelRequest) -> ServiceCall {
    ServiceCall::mutate(
        "cancel_knowledge_operation",
        "service_cancel_knowledge_operation",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Atomically saves one dictionary term and returns the current dictionary.
/// 原子保存一个词典词语并返回当前词典。
/// Parameters: request is the exact shared KnowledgeCustomWordRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeCustomWordRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn upsert_knowledge_custom_word(request: KnowledgeCustomWordRequest) -> ServiceCall {
    ServiceCall::mutate(
        "upsert_knowledge_custom_word",
        "service_upsert_knowledge_custom_word",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Reads a head-bound logical directory page without loading bodies.
/// 读取绑定变更头的逻辑目录页，不加载正文。
/// Parameters: request is the exact shared KnowledgeLibraryDocumentListRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeLibraryDocumentListRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn list_managed_knowledge_documents(
    request: KnowledgeLibraryDocumentListRequest,
) -> ServiceCall {
    ServiceCall::load(
        "list_managed_knowledge_documents",
        "service_list_knowledge_library_documents",
        json!({"request":request}),
        Slot::KnowledgeDocuments,
    )
}

/// Reads folders from the same bounded head-bound directory page.
/// 从相同有界且绑定变更头的目录页读取目录。
/// Parameters: request is the exact shared KnowledgeLibraryDocumentListRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeLibraryDocumentListRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
#[allow(
    dead_code,
    reason = "Complete one-to-one WebUI request mirror; the directory view uses the combined entry endpoint."
)]
pub fn list_managed_knowledge_document_folders(
    request: KnowledgeLibraryDocumentListRequest,
) -> ServiceCall {
    ServiceCall::load(
        "list_managed_knowledge_document_folders",
        "service_list_knowledge_library_folders",
        json!({"request":request}),
        Slot::KnowledgeFolders,
    )
}

/// Reads the exact current document selector and inclusive body window.
/// 读取精确当前文档选择器及包含边界正文窗口。
/// Parameters: request is the exact shared KnowledgeDocumentReadRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeDocumentReadRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn read_managed_knowledge_document(request: KnowledgeDocumentReadRequest) -> ServiceCall {
    ServiceCall::load(
        "read_managed_knowledge_document",
        "service_read_knowledge_library_document",
        json!({"request":request}),
        Slot::KnowledgeDocument,
    )
}

/// Searches one library by its own mode with explicit partial failures.
/// 按单个库自身模式检索，并明确返回部分失败。
/// Parameters: request is the exact shared KnowledgeLibrarySearchRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeLibrarySearchRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn search_managed_knowledge_library(request: KnowledgeLibrarySearchRequest) -> ServiceCall {
    ServiceCall::load(
        "search_managed_knowledge_library",
        "service_search_knowledge_library",
        json!({"request":request}),
        Slot::KnowledgeSearch,
    )
}

/// Reads a bounded immutable history summary page.
/// 读取有界不可变历史摘要页。
/// Parameters: request is the exact shared KnowledgeChangeListRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeChangeListRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn list_knowledge_changes(request: KnowledgeChangeListRequest) -> ServiceCall {
    ServiceCall::load(
        "list_knowledge_changes",
        "service_list_knowledge_changes",
        json!({"request":request}),
        Slot::KnowledgeChanges,
    )
}

/// Reads bounded ordered pre/post images for one immutable change.
/// 读取单个不可变变更的有界有序前后镜像。
/// Parameters: request is the exact shared KnowledgeChangeGetRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeChangeGetRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn get_knowledge_change(request: KnowledgeChangeGetRequest) -> ServiceCall {
    ServiceCall::load(
        "get_knowledge_change",
        "service_get_knowledge_change",
        json!({"request":request}),
        Slot::KnowledgeChange,
    )
}

/// Reads a historical body side without substituting current content.
/// 读取历史正文侧别，不替换为当前内容。
/// Parameters: request is the exact shared KnowledgeChangeDocumentReadRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeChangeDocumentReadRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn read_knowledge_change_document(request: KnowledgeChangeDocumentReadRequest) -> ServiceCall {
    ServiceCall::load(
        "read_knowledge_change_document",
        "service_read_knowledge_change_document",
        json!({"request":request}),
        Slot::KnowledgeDocument,
    )
}

/// Restores the human-confirmed history point as a new audited change.
/// 将人工确认历史点还原为新审计变更。
/// Parameters: request is the exact shared KnowledgeRestoreRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeRestoreRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn restore_knowledge_library(request: KnowledgeRestoreRequest) -> ServiceCall {
    ServiceCall::mutate(
        "restore_knowledge_library",
        "service_restore_knowledge_library",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Reads active/desired spaces, backlog and bounded failure details.
/// 读取活动及期望空间、积压与有界失败详情。
/// Parameters: request is the exact shared KnowledgeEmbeddingStatusRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeEmbeddingStatusRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn get_knowledge_embedding_status(request: KnowledgeEmbeddingStatusRequest) -> ServiceCall {
    ServiceCall::load(
        "get_knowledge_embedding_status",
        "service_get_knowledge_embedding_status",
        json!({"request":request}),
        Slot::KnowledgeEmbeddings,
    )
}

/// Retries current failed hashes for all or explicitly selected documents.
/// 为全部或显式选定文档重试当前失败哈希。
/// Parameters: request is the exact shared KnowledgeEmbeddingRetryRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeEmbeddingRetryRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn retry_knowledge_embeddings(request: KnowledgeEmbeddingRetryRequest) -> ServiceCall {
    ServiceCall::mutate(
        "retry_knowledge_embeddings",
        "service_retry_knowledge_embeddings",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Atomically exports current content and configuration without history or vectors.
/// 原子导出当前内容及配置，不含历史或向量。
/// Parameters: request is the exact shared KnowledgePackageExportRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgePackageExportRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn export_knowledge_package(request: KnowledgePackageExportRequest) -> ServiceCall {
    ServiceCall::mutate(
        "export_knowledge_package",
        "service_export_knowledge_package",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Validates an archive and returns the exact frozen byte identity.
/// 验证归档并返回精确冻结字节身份。
/// Parameters: request is the exact shared KnowledgePackageInspectRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgePackageInspectRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn inspect_knowledge_package(request: KnowledgePackageInspectRequest) -> ServiceCall {
    ServiceCall {
        function: "inspect_knowledge_package",
        method: "service_inspect_knowledge_package",
        params: json!({"request":request}),
        kind: CallKind::Preview(Preview::KnowledgePackage),
    }
}

/// Imports reviewed bytes into a new independent library only.
/// 仅将已审阅字节导入新独立知识库。
/// Parameters: request is the exact shared KnowledgePackageImportRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgePackageImportRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn import_knowledge_package(request: KnowledgePackageImportRequest) -> ServiceCall {
    ServiceCall::mutate(
        "import_knowledge_package",
        "service_import_knowledge_package",
        json!({"request":request}),
        LIBRARY,
    )
}

/// Reads a directory page after live project authorization.
/// 实时项目授权后读取目录页。
/// Parameters: request is the exact shared KnowledgeDocumentListRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeDocumentListRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
#[allow(
    dead_code,
    reason = "Complete project-authorized WebUI request mirror; the manager uses its managed endpoint."
)]
pub fn list_knowledge_documents(request: KnowledgeDocumentListRequest) -> ServiceCall {
    ServiceCall::load(
        "list_knowledge_documents",
        "service_list_knowledge_documents",
        json!({"request":request}),
        Slot::KnowledgeDocuments,
    )
}

/// Reads an exact document window after live project authorization.
/// 实时项目授权后读取精确文档窗口。
/// Parameters: request is the exact shared KnowledgeProjectDocumentReadRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeProjectDocumentReadRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
#[allow(
    dead_code,
    reason = "Complete project-authorized WebUI request mirror; the manager uses its managed endpoint."
)]
pub fn read_knowledge_document(request: KnowledgeProjectDocumentReadRequest) -> ServiceCall {
    ServiceCall::load(
        "read_knowledge_document",
        "service_read_knowledge_document",
        json!({"request":request}),
        Slot::KnowledgeDocument,
    )
}

/// Searches the exact authorized project subset with per-library modes.
/// 按每库模式检索精确授权项目子集。
/// Parameters: request is the exact shared KnowledgeSearchRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 KnowledgeSearchRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
#[allow(
    dead_code,
    reason = "Complete project-authorized WebUI request mirror; the manager searches one selected library."
)]
pub fn search_knowledge(request: KnowledgeSearchRequest) -> ServiceCall {
    ServiceCall::load(
        "search_knowledge",
        "service_search_knowledge",
        json!({"request":request}),
        Slot::KnowledgeSearch,
    )
}

/// Persists only the exact session library selection after live authorization.
/// 实时授权后仅持久化精确会话库选择。
/// Parameters: request is the exact shared SessionKnowledgeLibrarySelectionRequest; no legacy fields are accepted.
/// 参数：request 为精确共享 SessionKnowledgeLibrarySelectionRequest；不接受旧字段。
/// Returns an unexecuted request; transport, read ownership and post-write refresh belong to the backend.
/// 返回未执行请求；传输、读取归属及写后刷新由后端负责。
pub fn set_session_knowledge_libraries(
    request: SessionKnowledgeLibrarySelectionRequest,
) -> ServiceCall {
    ServiceCall::mutate(
        "set_session_knowledge_libraries",
        "service_set_session_knowledge_libraries",
        json!({"request":request}),
        &[Slot::KnowledgeSelection],
    )
}

/// Lists the registry or one project's authorized subset.
/// 列出注册表或一个项目的授权子集。
/// Parameters: project_id: Option<&str> mirrors the WebUI arguments and optional values exactly.
/// 参数：project_id: Option<&str> 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn list_knowledge_libraries(project_id: Option<&str>) -> ServiceCall {
    ServiceCall::load(
        "list_knowledge_libraries",
        "service_list_knowledge_libraries",
        json!({"projectId":project_id}),
        if project_id.is_some() {
            Slot::KnowledgeAuthorizedLibraries
        } else {
            Slot::KnowledgeLibraries
        },
    )
}

/// Deletes the exact library and its derived configuration.
/// 删除精确知识库及其派生配置。
/// Parameters: library_id: &str mirrors the WebUI arguments and optional values exactly.
/// 参数：library_id: &str 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn delete_knowledge_library(library_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "delete_knowledge_library",
        "service_delete_knowledge_library",
        json!({"libraryId":library_id}),
        LIBRARY,
    )
}

/// Lists exact project grants for one library.
/// 列出单个库的精确项目授权。
/// Parameters: library_id: &str mirrors the WebUI arguments and optional values exactly.
/// 参数：library_id: &str 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn list_knowledge_project_bindings(library_id: &str) -> ServiceCall {
    ServiceCall::load(
        "list_knowledge_project_bindings",
        "service_list_knowledge_project_bindings",
        json!({"libraryId":library_id}),
        Slot::KnowledgeBindings,
    )
}

/// Lists exact library grants for one project.
/// 列出单个项目的精确知识库授权。
/// Parameters: project_id: &str mirrors the WebUI arguments and optional values exactly.
/// 参数：project_id: &str 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn list_project_knowledge_bindings(project_id: &str) -> ServiceCall {
    ServiceCall::load(
        "list_project_knowledge_bindings",
        "service_list_project_knowledge_bindings",
        json!({"projectId":project_id}),
        Slot::ProjectKnowledgeBindings,
    )
}

/// Atomically removes one dictionary term without replacing concurrent entries.
/// 原子移除一个词典词语，不替换并发条目。
/// Parameters: library_id: &str, word: &str mirrors the WebUI arguments and optional values exactly.
/// 参数：library_id: &str, word: &str 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn remove_knowledge_custom_word(library_id: &str, word: &str) -> ServiceCall {
    ServiceCall::mutate(
        "remove_knowledge_custom_word",
        "service_remove_knowledge_custom_word",
        json!({"libraryId":library_id,"word":word}),
        LIBRARY,
    )
}

/// Queues a durable derived-index rebuild.
/// 排队持久派生索引重建。
/// Parameters: library_id: &str mirrors the WebUI arguments and optional values exactly.
/// 参数：library_id: &str 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn rebuild_knowledge_index(library_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "rebuild_knowledge_index",
        "service_rebuild_knowledge_index",
        json!({"libraryId":library_id}),
        LIBRARY,
    )
}

/// Repairs derived indexes through the same resumable rebuild.
/// 通过相同可恢复重建修复派生索引。
/// Parameters: library_id: &str mirrors the WebUI arguments and optional values exactly.
/// 参数：library_id: &str 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn repair_knowledge_index(library_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "repair_knowledge_index",
        "service_repair_knowledge_index",
        json!({"libraryId":library_id}),
        LIBRARY,
    )
}

/// Reads current authoritative/derived consistency without scheduling work.
/// 读取当前权威及派生一致性，不调度工作。
/// Parameters: library_id: &str mirrors the WebUI arguments and optional values exactly.
/// 参数：library_id: &str 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn get_knowledge_index_integrity(library_id: &str) -> ServiceCall {
    ServiceCall::load(
        "get_knowledge_index_integrity",
        "service_get_knowledge_index_integrity",
        json!({"libraryId":library_id}),
        Slot::KnowledgeIntegrity,
    )
}

/// Lists the library-owned custom dictionary.
/// 列出库拥有的自定义词典。
/// Parameters: library_id: &str mirrors the WebUI arguments and optional values exactly.
/// 参数：library_id: &str 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn list_knowledge_custom_words(library_id: &str) -> ServiceCall {
    ServiceCall::load(
        "list_knowledge_custom_words",
        "service_list_knowledge_custom_words",
        json!({"libraryId":library_id}),
        Slot::KnowledgeWords,
    )
}

/// Browses ordinary package files through the host's directory picker.
/// 通过宿主目录选择器浏览普通套件文件。
/// Parameters: path: Option<&str> mirrors the WebUI arguments and optional values exactly.
/// 参数：path: Option<&str> 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn browse_knowledge_packages(path: Option<&str>) -> ServiceCall {
    ServiceCall::load(
        "browse_knowledge_packages",
        "service_browse_knowledge_packages",
        json!({"path":path}),
        Slot::KnowledgePackages,
    )
}

/// Reads canonical selection and its currently authorized effective libraries.
/// 读取规范选择及其当前授权有效知识库。
/// Parameters: conversation_id: &str mirrors the WebUI arguments and optional values exactly.
/// 参数：conversation_id: &str 精确镜像 WebUI 参数及可选值。
/// Returns an unexecuted request with explicit read or mutation routing.
/// 返回具有明确读取或变更路由的未执行请求。
pub fn get_session_knowledge_libraries(conversation_id: &str) -> ServiceCall {
    ServiceCall::load(
        "get_session_knowledge_libraries",
        "service_get_session_knowledge_libraries",
        json!({"conversationId":conversation_id}),
        Slot::KnowledgeSelection,
    )
}

/// Freezes the inspected full archive fingerprint into an independent new-library import.
/// 将已检查完整归档指纹冻结为独立新库导入。
/// Parameters: inspection is the exact preview response; missing current-format fields reject confirmation.
/// 参数：inspection 为精确预检响应；缺少当前格式字段时拒绝确认。
/// Returns only the prepared current import, without a replace-existing branch.
/// 仅返回预备当前导入，不含覆盖既有库分支。
pub fn confirmed_package_import(inspection: &serde_json::Value) -> Option<ServiceCall> {
    // The manifest alone owns the human-editable suggested name.
    // 仅清单拥有可由人工编辑的建议名称。
    let name = inspection.pointer("/manifest/name")?.as_str()?;
    Some(import_knowledge_package(KnowledgePackageImportRequest {
        archive_path: inspection.get("archivePath")?.as_str()?.to_string(),
        expected_sha256: inspection.get("sha256")?.as_str()?.to_string(),
        name: name.to_string(),
    }))
}
