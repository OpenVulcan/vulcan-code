import { browseProjectDirectories } from "@/service/project-actions-service";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type {
  KnowledgeDocumentSummary,
  KnowledgeDocumentContent,
  KnowledgePackageExportResult,
  KnowledgePackageFileListing,
  KnowledgePackageImportResult,
  KnowledgePackageInspection,
  KnowledgeImportJobSummary,
  KnowledgeIndexIntegritySummary,
  KnowledgeLibrarySummary,
  KnowledgeLibrarySettingsRequest,
  KnowledgeSourceSummary,
  KnowledgeProjectBinding,
  KnowledgeSearchResult,
  ProjectDirectoryListing,
  SessionKnowledgeLibrarySelectionSummary,
} from "@/types";

// knowledgeProjectBindingListeners contains the in-process subscribers that must refresh after one project binding changes.
// knowledgeProjectBindingListeners 保存项目绑定变更后必须刷新的进程内订阅者。
const knowledgeProjectBindingListeners = new Set<(projectId: string) => void>();

// notifyKnowledgeProjectBindingsChanged publishes one successfully persisted project-binding revision.
// notifyKnowledgeProjectBindingsChanged 发布一次已成功持久化的项目绑定修订。
// Parameters: projectId is the exact project whose authorization changed. Returns: nothing.
// 参数：projectId 是授权发生变化的精确项目。返回值：无。
export function notifyKnowledgeProjectBindingsChanged(projectId: string): void {
  // listeners is a stable snapshot so subscription changes during notification affect only later revisions.
  // listeners 是稳定快照，使通知期间的订阅变化仅影响后续修订。
  const listeners = [...knowledgeProjectBindingListeners];
  listeners.forEach((listener) => {
    try {
      listener(projectId);
    } catch (error) {
      console.error("[knowledge-base] project binding listener failed", error);
    }
  });
}

// subscribeKnowledgeProjectBindingsChanged registers one local binding-revision listener.
// subscribeKnowledgeProjectBindingsChanged 注册一个本地绑定修订监听器。
// Parameters: listener receives the exact changed project ID. Returns: an idempotent unsubscribe callback.
// 参数：listener 接收发生变化的精确项目标识。返回值：一个幂等的取消订阅回调。
export function subscribeKnowledgeProjectBindingsChanged(listener: (projectId: string) => void): () => void {
  knowledgeProjectBindingListeners.add(listener);
  return () => {
    knowledgeProjectBindingListeners.delete(listener);
  };
}

// listKnowledgeLibraries loads all libraries or one project's authorized subset.
// listKnowledgeLibraries 加载全部知识库或单个项目的授权子集。
export async function listKnowledgeLibraries(projectId?: string): Promise<KnowledgeLibrarySummary[]> {
  if (!hasServiceRuntime()) {
    return [];
  }
  return serviceClient.request<KnowledgeLibrarySummary[]>("service_list_knowledge_libraries", { projectId: projectId || null });
}

// createKnowledgeLibrary creates one host-owned isolated library.
// createKnowledgeLibrary 创建一个宿主持有的隔离知识库。
export async function createKnowledgeLibrary(
  name: string,
  description: string,
  projectIds: string[],
  settings: KnowledgeLibrarySettingsRequest = {},
): Promise<KnowledgeLibrarySummary> {
  return serviceClient.request<KnowledgeLibrarySummary>("service_create_knowledge_library", {
    request: { name, description, projectIds, settings },
  });
}

// updateKnowledgeLibrarySettings persists a library-level policy without mutating source documents.
// updateKnowledgeLibrarySettings 持久化资料库级策略且不修改原始文档。
export async function updateKnowledgeLibrarySettings(
  libraryId: string,
  settings: KnowledgeLibrarySettingsRequest,
): Promise<KnowledgeLibrarySummary> {
  return serviceClient.request<KnowledgeLibrarySummary>("service_set_knowledge_library_settings", {
    request: { libraryId, settings },
  });
}

// renameKnowledgeLibrary updates user-visible metadata without changing physical identity.
// renameKnowledgeLibrary 更新用户可见元数据且不改变物理身份。
export async function renameKnowledgeLibrary(libraryId: string, name: string, description: string): Promise<KnowledgeLibrarySummary> {
  return serviceClient.request<KnowledgeLibrarySummary>("service_rename_knowledge_library", {
    request: { libraryId, name, description },
  });
}

// deleteKnowledgeLibrary permanently removes one isolated library.
// deleteKnowledgeLibrary 永久删除一个隔离知识库。
export async function deleteKnowledgeLibrary(libraryId: string): Promise<void> {
  await serviceClient.request("service_delete_knowledge_library", { libraryId });
}

// listKnowledgeProjectBindings loads the exact authorization rows for one library.
// listKnowledgeProjectBindings 加载一个知识库的精确授权记录。
export async function listKnowledgeProjectBindings(libraryId: string): Promise<KnowledgeProjectBinding[]> {
  return serviceClient.request<KnowledgeProjectBinding[]>("service_list_knowledge_project_bindings", { libraryId });
}

// listProjectKnowledgeBindings loads all active-library bindings for one exact project.
// listProjectKnowledgeBindings 加载一个精确项目的全部活动知识库绑定。
export async function listProjectKnowledgeBindings(projectId: string): Promise<KnowledgeProjectBinding[]> {
  return serviceClient.request<KnowledgeProjectBinding[]>("service_list_project_knowledge_bindings", { projectId });
}

// listKnowledgeSources loads all durable Markdown roots attached to one library.
// listKnowledgeSources 加载关联到一个知识库的全部持久化 Markdown 根目录。
// Parameters: libraryId is the exact physical library identity. Returns: ordered source summaries.
// 参数：libraryId 是精确物理知识库身份。返回值：有序来源摘要。
export async function listKnowledgeSources(libraryId: string): Promise<KnowledgeSourceSummary[]> {
  return serviceClient.request<KnowledgeSourceSummary[]>("service_list_knowledge_sources", { libraryId });
}

// setKnowledgeProjectBinding updates one project authorization and inheritance default.
// setKnowledgeProjectBinding 更新一条项目授权与继承默认值。
export async function setKnowledgeProjectBinding(libraryId: string, projectId: string, authorized: boolean, defaultEnabled: boolean): Promise<void> {
  await serviceClient.request("service_set_knowledge_project_binding", {
    request: { libraryId, projectId, authorized, defaultEnabled },
  });
}

// importKnowledgeDirectory starts a detached recursive Markdown import and returns its durable job.
// importKnowledgeDirectory 启动分离式递归 Markdown 导入并返回其持久化任务。
export async function importKnowledgeDirectory(libraryId: string, sourceRoot: string, sourceId: string | null, confirmation: import("@/types").KnowledgeFileConfirmation): Promise<KnowledgeImportJobSummary> {
  return serviceClient.request<KnowledgeImportJobSummary>("service_import_knowledge_directory", {
    request: { libraryId, sourceRoot, sourceId, previewFingerprint: confirmation.fingerprint, selectedFileKeys: confirmation.selectedKeys },
  });
}

// cancelKnowledgeImport requests cancellation only for the exact active knowledge operation.
// cancelKnowledgeImport 仅请求取消精确匹配的活动知识库操作。
export async function cancelKnowledgeImport(libraryId: string, jobId: string): Promise<boolean> {
  const result = await serviceClient.request<{ cancelRequested: boolean }>("service_cancel_knowledge_import", { libraryId, jobId });
  return result.cancelRequested;
}

// getKnowledgeImportJob loads one dedicated operation snapshot without refreshing the workbench.
// getKnowledgeImportJob 加载一份专用操作快照且不刷新工作台。
export async function getKnowledgeImportJob(libraryId: string): Promise<KnowledgeImportJobSummary | null> {
  return serviceClient.request<KnowledgeImportJobSummary | null>("service_get_knowledge_import_job", { libraryId });
}

// rebuildKnowledgeIndex starts rebuilding the derived Jieba FTS index from authoritative chunks.
// rebuildKnowledgeIndex 启动从权威切片重建派生 Jieba FTS 索引。
export async function rebuildKnowledgeIndex(libraryId: string): Promise<KnowledgeImportJobSummary> {
  return serviceClient.request<KnowledgeImportJobSummary>("service_rebuild_knowledge_index", { libraryId });
}

// repairKnowledgeIndex starts a non-destructive completion of missing or mismatched FTS rows.
// repairKnowledgeIndex 启动非破坏式补全缺失或不匹配的 FTS 记录。
export async function repairKnowledgeIndex(libraryId: string): Promise<KnowledgeImportJobSummary> {
  return serviceClient.request<KnowledgeImportJobSummary>("service_repair_knowledge_index", { libraryId });
}

// getKnowledgeIndexIntegrity loads exact revision and physical-row integrity diagnostics.
// getKnowledgeIndexIntegrity 加载精确版本与物理记录完整性诊断。
export async function getKnowledgeIndexIntegrity(libraryId: string): Promise<KnowledgeIndexIntegritySummary> {
  return serviceClient.request<KnowledgeIndexIntegritySummary>("service_get_knowledge_index_integrity", { libraryId });
}

// listManagedKnowledgeDocuments loads one library page without applying project or session ACLs.
// listManagedKnowledgeDocuments 加载一个知识库分页，不应用项目或会话 ACL。
export async function listManagedKnowledgeDocuments(
  libraryId: string,
  folderPath: string | null = null,
  limit = 100,
  offset = 0,
): Promise<KnowledgeDocumentSummary[]> {
  return serviceClient.request<KnowledgeDocumentSummary[]>("service_list_knowledge_library_documents", {
    request: { libraryId, folderPath, limit, offset },
  });
}

// listManagedKnowledgeDocumentFolders loads only distinct logical paths for lazy tree navigation.
// listManagedKnowledgeDocumentFolders 仅加载用于延迟目录树导航的唯一逻辑路径。
export async function listManagedKnowledgeDocumentFolders(libraryId: string): Promise<string[]> {
  return serviceClient.request<string[]>("service_list_knowledge_library_folders", { libraryId });
}

// readManagedKnowledgeDocument reads one library document without applying project or session ACLs.
// readManagedKnowledgeDocument 读取一个知识库文档，不应用项目或会话 ACL。
export async function readManagedKnowledgeDocument(
  libraryId: string,
  documentId: string,
  corpus: "raw" | "display" = "raw",
): Promise<KnowledgeDocumentContent> {
  return serviceClient.request<KnowledgeDocumentContent>("service_read_knowledge_library_document", { libraryId, documentId, corpus });
}

// searchManagedKnowledgeLibrary searches only the library selected in the management surface.
// searchManagedKnowledgeLibrary 仅检索管理界面中选中的知识库。
export async function searchManagedKnowledgeLibrary(
  libraryId: string,
  query: string,
  retrievalMode?: "raw_bm25" | "display_bm25" | "display_hybrid",
): Promise<KnowledgeSearchResult> {
  return serviceClient.request<KnowledgeSearchResult>("service_search_knowledge_library", {
    request: { libraryId, query, limit: 20, retrievalMode: retrievalMode ?? null },
  });
}

// buildKnowledgeDisplayIndex runs the fixed-prompt display-corpus compiler for one library.
// buildKnowledgeDisplayIndex 为一个资料库运行固定提示词显示库编译器。
export async function buildKnowledgeDisplayIndex(libraryId: string, confirmation: import("@/types").KnowledgeFileConfirmation): Promise<KnowledgeImportJobSummary> {
  return serviceClient.request<KnowledgeImportJobSummary>("service_build_knowledge_display_index", { libraryId, confirmation });
}

// previewKnowledgeImport scans without importing; returns all rows and their exact confirmation fingerprint.
// previewKnowledgeImport 扫描但不导入，返回全部记录及精确确认指纹。
// Parameters: libraryId owns the scan; sourceRoot/sourceId select the directory; signal cancels read-only waiting.
// 参数：libraryId 为扫描归属，sourceRoot/sourceId 选择目录，signal 取消只读等待。
export async function previewKnowledgeImport(libraryId: string, sourceRoot: string, sourceId: string | null, signal?: AbortSignal): Promise<import("@/types").KnowledgeFilePreview> {
  return serviceClient.request("service_preview_knowledge_import", { libraryId, sourceRoot, sourceId }, { signal });
}

// previewKnowledgeDisplay inspects current source and generated hashes without starting a build.
// previewKnowledgeDisplay 检查当前来源与生成哈希，不启动构建。
// Parameters: libraryId owns the corpus; signal cancels waiting. Returns the current confirmation snapshot.
// 参数：libraryId 为内容库归属，signal 取消等待。返回当前确认快照。
export async function previewKnowledgeDisplay(libraryId: string, signal?: AbortSignal): Promise<import("@/types").KnowledgeFilePreview> {
  return serviceClient.request("service_preview_knowledge_display", { libraryId }, { signal });
}

// updateKnowledgeIgnoredFiles explicitly adds/removes exact files; returns durable rules without deleting content.
// updateKnowledgeIgnoredFiles 明确增删精确文件，返回持久规则，不删除内容。
export async function updateKnowledgeIgnoredFiles(libraryId: string, files: import("@/types").KnowledgeIgnoredFile[], ignored: boolean): Promise<import("@/types").KnowledgeIgnoredFile[]> {
  return serviceClient.request("service_update_knowledge_ignored_files", { libraryId, files, ignored });
}

// buildKnowledgeVectorIndex compiles vectors for the active curated display generation.
// buildKnowledgeVectorIndex 为当前活动整理显示代次生成向量索引。
export async function buildKnowledgeVectorIndex(libraryId: string): Promise<KnowledgeImportJobSummary> {
  return serviceClient.request<KnowledgeImportJobSummary>("service_build_knowledge_vector_index", { libraryId });
}

// exportKnowledgePackage writes one complete ready library into a timestamped .vkb archive.
// exportKnowledgePackage 将一个完整且就绪的知识库写入带时间戳的 .vkb 归档。
export async function exportKnowledgePackage(libraryId: string, destinationDirectory: string): Promise<KnowledgePackageExportResult> {
  return serviceClient.request<KnowledgePackageExportResult>("service_export_knowledge_package", {
    request: { libraryId, destinationDirectory },
  });
}

// inspectKnowledgePackage validates one selected .vkb archive without mutating library state.
// inspectKnowledgePackage 校验一个选中的 .vkb 归档且不改变知识库状态。
export async function inspectKnowledgePackage(archivePath: string): Promise<KnowledgePackageInspection> {
  return serviceClient.request<KnowledgePackageInspection>("service_inspect_knowledge_package", {
    request: { archivePath },
  });
}

// importKnowledgePackage creates or explicitly replaces one library from a validated package.
// importKnowledgePackage 从已校验包新建或显式替换一个知识库。
export async function importKnowledgePackage(
  archivePath: string,
  name: string,
  strategy: "create" | "replace",
  targetLibraryId?: string | null,
): Promise<KnowledgePackageImportResult> {
  return serviceClient.request<KnowledgePackageImportResult>("service_import_knowledge_package", {
    request: { archivePath, name, strategy, targetLibraryId: targetLibraryId ?? null },
  });
}

// browseKnowledgePackages lists directories and ordinary .vkb files for the import picker.
// browseKnowledgePackages 为导入选择器列出目录和普通 .vkb 文件。
export async function browseKnowledgePackages(path: string | null): Promise<KnowledgePackageFileListing> {
  return serviceClient.request<KnowledgePackageFileListing>("service_browse_knowledge_packages", { path });
}

// browseKnowledgeSourceDirectories reuses the host directory browser contract.
// browseKnowledgeSourceDirectories 复用宿主目录浏览契约。
export function browseKnowledgeSourceDirectories(path: string | null): Promise<ProjectDirectoryListing> {
  return browseProjectDirectories(path);
}

// getSessionKnowledgeLibraries loads one conversation's persisted and effective selection.
// getSessionKnowledgeLibraries 加载一个会话持久化且生效的知识库选择。
export async function getSessionKnowledgeLibraries(conversationId: string): Promise<SessionKnowledgeLibrarySelectionSummary> {
  return serviceClient.request<SessionKnowledgeLibrarySelectionSummary>("service_get_session_knowledge_libraries", { conversationId });
}

// setSessionKnowledgeLibraries persists one project-authorized conversation selection.
// setSessionKnowledgeLibraries 持久化一份经过项目授权的会话选择。
export async function setSessionKnowledgeLibraries(
  projectId: string,
  conversationId: string,
  mode: "inherit" | "explicit",
  libraryIds: string[],
  retrievalMode?: "raw_bm25" | "display_bm25" | "display_hybrid",
): Promise<void> {
  await serviceClient.request("service_set_session_knowledge_libraries", {
    request: { projectId, conversationId, mode, libraryIds, retrievalMode: retrievalMode ?? null },
  });
}
