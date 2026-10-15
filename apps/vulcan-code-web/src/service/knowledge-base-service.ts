import { serviceClient } from "@/service/webui-service-runtime";
import type * as Knowledge from "@/types/knowledge-base";

// Subscribers refresh the exact project's library ceiling after a confirmed binding change.
// 订阅者在确认绑定变更后刷新精确项目知识库上限。
const knowledgeProjectBindingListeners = new Set<(projectId: string) => void>();

// Publishes projectId after successful persistence; returns nothing and isolates subscriber failures.
// 成功持久化后发布 projectId；无返回值，隔离订阅者失败。
export function notifyKnowledgeProjectBindingsChanged(projectId: string): void {
  // A stable subscriber snapshot gives each publication deterministic ownership.
  // 稳定订阅者快照使每次发布具有确定归属。
  const listeners = [...knowledgeProjectBindingListeners];
  for (const listener of listeners) {
    try { listener(projectId); }
    catch (error) { console.error("[knowledge-base] project binding listener failed", error); }
  }
}

// Registers listener for exact project IDs; returns an idempotent unsubscribe callback.
// 为精确项目身份注册 listener；返回幂等取消订阅回调。
export function subscribeKnowledgeProjectBindingsChanged(listener: (projectId: string) => void): () => void {
  knowledgeProjectBindingListeners.add(listener);
  return () => { knowledgeProjectBindingListeners.delete(listener); };
}

// Creates an empty library using the explicit mode and admitted model route.
// 以显式模式及准入模型路由创建空库。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeLibrarySummary or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeLibrarySummary 或明确服务错误。
export async function createKnowledgeLibrary(request: Knowledge.KnowledgeLibraryCreateRequest): Promise<Knowledge.KnowledgeLibrarySummary> {
  return serviceClient.request<Knowledge.KnowledgeLibrarySummary>("service_create_knowledge_library", { request });
}

// Updates only the human library name and purpose.
// 仅更新人工知识库名称及用途。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeLibrarySummary or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeLibrarySummary 或明确服务错误。
export async function updateKnowledgeLibrarySettings(request: Knowledge.KnowledgeLibrarySettingsUpdateRequest): Promise<Knowledge.KnowledgeLibrarySummary> {
  return serviceClient.request<Knowledge.KnowledgeLibrarySummary>("service_set_knowledge_library_settings", { request });
}

// Saves the exact library name and purpose.
// 保存精确知识库名称及用途。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeLibrarySummary or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeLibrarySummary 或明确服务错误。
export async function renameKnowledgeLibrary(request: Knowledge.KnowledgeLibraryRenameRequest): Promise<Knowledge.KnowledgeLibrarySummary> {
  return serviceClient.request<Knowledge.KnowledgeLibrarySummary>("service_rename_knowledge_library", { request });
}

// Persists the explicit target mode and returns its durable operation.
// 持久化显式目标模式并返回其持久操作。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeOperationSummary or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeOperationSummary 或明确服务错误。
export async function setKnowledgeIndexMode(request: Knowledge.KnowledgeIndexModeRequestPayload): Promise<Knowledge.KnowledgeOperationSummary> {
  return serviceClient.request<Knowledge.KnowledgeOperationSummary>("service_set_knowledge_index_mode", { request });
}

// Changes one project's authorization and inherited enablement.
// 变更一个项目的授权及继承启用。
// Parameters: request is the exact current service DTO. Returns the authoritative void or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 void 或明确服务错误。
export async function setKnowledgeProjectBinding(request: Knowledge.KnowledgeProjectBindingRequest): Promise<void> {
  return serviceClient.request<void>("service_set_knowledge_project_binding", { request });
}

// Reads the exact stable operation or explicitly discovers the latest.
// 读取精确稳定操作，或显式发现最新操作。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeOperationSummary | null or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeOperationSummary | null 或明确服务错误。
export async function getKnowledgeOperation(request: Knowledge.KnowledgeOperationRequest): Promise<Knowledge.KnowledgeOperationSummary | null> {
  return serviceClient.request<Knowledge.KnowledgeOperationSummary | null>("service_get_knowledge_operation", { request });
}

// Cancels one exact operation and returns whether it transitioned.
// 取消一个精确操作并返回是否发生过渡。
// Parameters: request is the exact current service DTO. Returns the authoritative boolean or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 boolean 或明确服务错误。
export async function cancelKnowledgeOperation(request: Knowledge.KnowledgeOperationCancelRequest): Promise<boolean> {
  return serviceClient.request<boolean>("service_cancel_knowledge_operation", { request });
}

// Atomically saves one dictionary term and returns the current dictionary.
// 原子保存一个词典词语并返回当前词典。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeCustomWord[] or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeCustomWord[] 或明确服务错误。
export async function upsertKnowledgeCustomWord(request: Knowledge.KnowledgeCustomWordRequest): Promise<Knowledge.KnowledgeCustomWord[]> {
  return serviceClient.request<Knowledge.KnowledgeCustomWord[]>("service_upsert_knowledge_custom_word", { request });
}

// Reads a head-bound logical directory page without loading bodies.
// 读取绑定变更头的逻辑目录页，不加载正文。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeListPage or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeListPage 或明确服务错误。
export async function listManagedKnowledgeDocuments(request: Knowledge.KnowledgeLibraryDocumentListRequest): Promise<Knowledge.KnowledgeListPage> {
  return serviceClient.request<Knowledge.KnowledgeListPage>("service_list_knowledge_library_documents", { request });
}

// Reads folders from the same bounded head-bound directory page.
// 从相同有界且绑定变更头的目录页读取目录。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeListPage or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeListPage 或明确服务错误。
export async function listManagedKnowledgeDocumentFolders(request: Knowledge.KnowledgeLibraryDocumentListRequest): Promise<Knowledge.KnowledgeListPage> {
  return serviceClient.request<Knowledge.KnowledgeListPage>("service_list_knowledge_library_folders", { request });
}

// Reads the exact current document selector and inclusive body window.
// 读取精确当前文档选择器及包含边界正文窗口。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeDocumentContent or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeDocumentContent 或明确服务错误。
export async function readManagedKnowledgeDocument(request: Knowledge.KnowledgeDocumentReadRequest): Promise<Knowledge.KnowledgeDocumentContent> {
  return serviceClient.request<Knowledge.KnowledgeDocumentContent>("service_read_knowledge_library_document", { request });
}

// Searches one library by its own mode with explicit partial failures.
// 按单个库自身模式检索，并明确返回部分失败。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeSearchResult or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeSearchResult 或明确服务错误。
export async function searchManagedKnowledgeLibrary(request: Knowledge.KnowledgeLibrarySearchRequest): Promise<Knowledge.KnowledgeSearchResult> {
  return serviceClient.request<Knowledge.KnowledgeSearchResult>("service_search_knowledge_library", { request });
}

// Reads a bounded immutable history summary page.
// 读取有界不可变历史摘要页。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeChange[] or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeChange[] 或明确服务错误。
export async function listKnowledgeChanges(request: Knowledge.KnowledgeChangeListRequest): Promise<Knowledge.KnowledgeChange[]> {
  return serviceClient.request<Knowledge.KnowledgeChange[]>("service_list_knowledge_changes", { request });
}

// Reads bounded ordered pre/post images for one immutable change.
// 读取单个不可变变更的有界有序前后镜像。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeChangeDetail or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeChangeDetail 或明确服务错误。
export async function getKnowledgeChange(request: Knowledge.KnowledgeChangeGetRequest): Promise<Knowledge.KnowledgeChangeDetail> {
  return serviceClient.request<Knowledge.KnowledgeChangeDetail>("service_get_knowledge_change", { request });
}

// Reads a historical body side without substituting current content.
// 读取历史正文侧别，不替换为当前内容。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeDocumentContent or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeDocumentContent 或明确服务错误。
export async function readKnowledgeChangeDocument(request: Knowledge.KnowledgeChangeDocumentReadRequest): Promise<Knowledge.KnowledgeDocumentContent> {
  return serviceClient.request<Knowledge.KnowledgeDocumentContent>("service_read_knowledge_change_document", { request });
}

// Restores the human-confirmed history point as a new audited change.
// 将人工确认历史点还原为新审计变更。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeWriteResult or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeWriteResult 或明确服务错误。
export async function restoreKnowledgeLibrary(request: Knowledge.KnowledgeRestoreRequest): Promise<Knowledge.KnowledgeWriteResult> {
  return serviceClient.request<Knowledge.KnowledgeWriteResult>("service_restore_knowledge_library", { request });
}

// Reads active/desired spaces, backlog and bounded failure details.
// 读取活动及期望空间、积压与有界失败详情。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeEmbeddingStatus or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeEmbeddingStatus 或明确服务错误。
export async function getKnowledgeEmbeddingStatus(request: Knowledge.KnowledgeEmbeddingStatusRequest): Promise<Knowledge.KnowledgeEmbeddingStatus> {
  return serviceClient.request<Knowledge.KnowledgeEmbeddingStatus>("service_get_knowledge_embedding_status", { request });
}

// Retries current failed hashes for all or explicitly selected documents.
// 为全部或显式选定文档重试当前失败哈希。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeOperationSummary or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeOperationSummary 或明确服务错误。
export async function retryKnowledgeEmbeddings(request: Knowledge.KnowledgeEmbeddingRetryRequest): Promise<Knowledge.KnowledgeOperationSummary> {
  return serviceClient.request<Knowledge.KnowledgeOperationSummary>("service_retry_knowledge_embeddings", { request });
}

// Atomically exports current content and configuration without history or vectors.
// 原子导出当前内容及配置，不含历史或向量。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgePackageExportResult or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgePackageExportResult 或明确服务错误。
export async function exportKnowledgePackage(request: Knowledge.KnowledgePackageExportRequest): Promise<Knowledge.KnowledgePackageExportResult> {
  return serviceClient.request<Knowledge.KnowledgePackageExportResult>("service_export_knowledge_package", { request });
}

// Validates an archive and returns the exact frozen byte identity.
// 验证归档并返回精确冻结字节身份。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgePackageInspection or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgePackageInspection 或明确服务错误。
export async function inspectKnowledgePackage(request: Knowledge.KnowledgePackageInspectRequest): Promise<Knowledge.KnowledgePackageInspection> {
  return serviceClient.request<Knowledge.KnowledgePackageInspection>("service_inspect_knowledge_package", { request });
}

// Imports reviewed bytes into a new independent library only.
// 仅将已审阅字节导入新独立知识库。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgePackageImportResult or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgePackageImportResult 或明确服务错误。
export async function importKnowledgePackage(request: Knowledge.KnowledgePackageImportRequest): Promise<Knowledge.KnowledgePackageImportResult> {
  return serviceClient.request<Knowledge.KnowledgePackageImportResult>("service_import_knowledge_package", { request });
}

// Reads a directory page after live project authorization.
// 实时项目授权后读取目录页。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeListPage or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeListPage 或明确服务错误。
export async function listKnowledgeDocuments(request: Knowledge.KnowledgeDocumentListRequest): Promise<Knowledge.KnowledgeListPage> {
  return serviceClient.request<Knowledge.KnowledgeListPage>("service_list_knowledge_documents", { request });
}

// Reads an exact document window after live project authorization.
// 实时项目授权后读取精确文档窗口。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeDocumentContent or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeDocumentContent 或明确服务错误。
export async function readKnowledgeDocument(request: Knowledge.KnowledgeProjectDocumentReadRequest): Promise<Knowledge.KnowledgeDocumentContent> {
  return serviceClient.request<Knowledge.KnowledgeDocumentContent>("service_read_knowledge_document", { request });
}

// Searches the exact authorized project subset with per-library modes.
// 按每库模式检索精确授权项目子集。
// Parameters: request is the exact current service DTO. Returns the authoritative KnowledgeSearchResult or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 KnowledgeSearchResult 或明确服务错误。
export async function searchKnowledge(request: Knowledge.KnowledgeSearchRequest): Promise<Knowledge.KnowledgeSearchResult> {
  return serviceClient.request<Knowledge.KnowledgeSearchResult>("service_search_knowledge", { request });
}

// Persists only the exact session library selection after live authorization.
// 实时授权后仅持久化精确会话库选择。
// Parameters: request is the exact current service DTO. Returns the authoritative SessionKnowledgeLibrarySelectionSummary or an explicit service error.
// 参数：request 为精确当前服务 DTO。返回权威 SessionKnowledgeLibrarySelectionSummary 或明确服务错误。
export async function setSessionKnowledgeLibraries(request: Knowledge.SessionKnowledgeLibrarySelectionRequest): Promise<Knowledge.SessionKnowledgeLibrarySelectionSummary> {
  return serviceClient.request<Knowledge.SessionKnowledgeLibrarySelectionSummary>("service_set_session_knowledge_libraries", { request });
}

// Lists the registry or one project's authorized subset.
// 列出注册表或一个项目的授权子集。
// Parameters: projectId?: string. Returns the authoritative KnowledgeLibrarySummary[] or an explicit service error.
// 参数：projectId?: string。返回权威 KnowledgeLibrarySummary[] 或明确服务错误。
export async function listKnowledgeLibraries(projectId?: string): Promise<Knowledge.KnowledgeLibrarySummary[]> {
  return serviceClient.request<Knowledge.KnowledgeLibrarySummary[]>("service_list_knowledge_libraries", { projectId: projectId ?? null });
}

// Deletes the exact library and its derived configuration.
// 删除精确知识库及其派生配置。
// Parameters: libraryId: string. Returns the authoritative void or an explicit service error.
// 参数：libraryId: string。返回权威 void 或明确服务错误。
export async function deleteKnowledgeLibrary(libraryId: string): Promise<void> {
  return serviceClient.request<void>("service_delete_knowledge_library", { libraryId });
}

// Lists exact project grants for one library.
// 列出单个库的精确项目授权。
// Parameters: libraryId: string. Returns the authoritative KnowledgeProjectBinding[] or an explicit service error.
// 参数：libraryId: string。返回权威 KnowledgeProjectBinding[] 或明确服务错误。
export async function listKnowledgeProjectBindings(libraryId: string): Promise<Knowledge.KnowledgeProjectBinding[]> {
  return serviceClient.request<Knowledge.KnowledgeProjectBinding[]>("service_list_knowledge_project_bindings", { libraryId });
}

// Lists exact library grants for one project.
// 列出单个项目的精确知识库授权。
// Parameters: projectId: string. Returns the authoritative KnowledgeProjectBinding[] or an explicit service error.
// 参数：projectId: string。返回权威 KnowledgeProjectBinding[] 或明确服务错误。
export async function listProjectKnowledgeBindings(projectId: string): Promise<Knowledge.KnowledgeProjectBinding[]> {
  return serviceClient.request<Knowledge.KnowledgeProjectBinding[]>("service_list_project_knowledge_bindings", { projectId });
}

// Atomically removes one dictionary term without replacing concurrent entries.
// 原子移除一个词典词语，不替换并发条目。
// Parameters: libraryId: string, word: string. Returns the authoritative KnowledgeCustomWord[] or an explicit service error.
// 参数：libraryId: string, word: string。返回权威 KnowledgeCustomWord[] 或明确服务错误。
export async function removeKnowledgeCustomWord(libraryId: string, word: string): Promise<Knowledge.KnowledgeCustomWord[]> {
  return serviceClient.request<Knowledge.KnowledgeCustomWord[]>("service_remove_knowledge_custom_word", { libraryId, word });
}

// Queues a durable derived-index rebuild.
// 排队持久派生索引重建。
// Parameters: libraryId: string. Returns the authoritative KnowledgeOperationSummary or an explicit service error.
// 参数：libraryId: string。返回权威 KnowledgeOperationSummary 或明确服务错误。
export async function rebuildKnowledgeIndex(libraryId: string): Promise<Knowledge.KnowledgeOperationSummary> {
  return serviceClient.request<Knowledge.KnowledgeOperationSummary>("service_rebuild_knowledge_index", { libraryId });
}

// Repairs derived indexes through the same resumable rebuild.
// 通过相同可恢复重建修复派生索引。
// Parameters: libraryId: string. Returns the authoritative KnowledgeOperationSummary or an explicit service error.
// 参数：libraryId: string。返回权威 KnowledgeOperationSummary 或明确服务错误。
export async function repairKnowledgeIndex(libraryId: string): Promise<Knowledge.KnowledgeOperationSummary> {
  return serviceClient.request<Knowledge.KnowledgeOperationSummary>("service_repair_knowledge_index", { libraryId });
}

// Reads current authoritative/derived consistency without scheduling work.
// 读取当前权威及派生一致性，不调度工作。
// Parameters: libraryId: string. Returns the authoritative KnowledgeIndexIntegritySummary or an explicit service error.
// 参数：libraryId: string。返回权威 KnowledgeIndexIntegritySummary 或明确服务错误。
export async function getKnowledgeIndexIntegrity(libraryId: string): Promise<Knowledge.KnowledgeIndexIntegritySummary> {
  return serviceClient.request<Knowledge.KnowledgeIndexIntegritySummary>("service_get_knowledge_index_integrity", { libraryId });
}

// Lists the library-owned custom dictionary.
// 列出库拥有的自定义词典。
// Parameters: libraryId: string. Returns the authoritative KnowledgeCustomWord[] or an explicit service error.
// 参数：libraryId: string。返回权威 KnowledgeCustomWord[] 或明确服务错误。
export async function listKnowledgeCustomWords(libraryId: string): Promise<Knowledge.KnowledgeCustomWord[]> {
  return serviceClient.request<Knowledge.KnowledgeCustomWord[]>("service_list_knowledge_custom_words", { libraryId });
}

// Browses ordinary package files through the host's directory picker.
// 通过宿主目录选择器浏览普通套件文件。
// Parameters: path: string | null. Returns the authoritative KnowledgePackageFileListing or an explicit service error.
// 参数：path: string | null。返回权威 KnowledgePackageFileListing 或明确服务错误。
export async function browseKnowledgePackages(path: string | null): Promise<Knowledge.KnowledgePackageFileListing> {
  return serviceClient.request<Knowledge.KnowledgePackageFileListing>("service_browse_knowledge_packages", { path });
}

// Reads canonical selection and its currently authorized effective libraries.
// 读取规范选择及其当前授权有效知识库。
// Parameters: conversationId: string. Returns the authoritative SessionKnowledgeLibrarySelectionSummary or an explicit service error.
// 参数：conversationId: string。返回权威 SessionKnowledgeLibrarySelectionSummary 或明确服务错误。
export async function getSessionKnowledgeLibraries(conversationId: string): Promise<Knowledge.SessionKnowledgeLibrarySelectionSummary> {
  return serviceClient.request<Knowledge.SessionKnowledgeLibrarySelectionSummary>("service_get_session_knowledge_libraries", { conversationId });
}
