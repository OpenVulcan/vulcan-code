import * as React from "react";
import "@/styles/knowledge.css";
import { KnowledgeSkippedDocuments } from "./knowledge-skipped-documents";
import { KnowledgeFileConfirmationDialog, type KnowledgeFileReviewTarget } from "./knowledge-file-confirmation-dialog";
import { BookOpen, Database, Download, FolderInput, FolderPen, Plus, RefreshCw, Search, Settings2, Trash2, Upload, Wrench } from "lucide-react";
import { toast } from "sonner";

import { ProjectFolderPickerDialog } from "@/components/project/folder-picker-dialog";
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction } from "@/components/ui/alert-dialog";
import { Input } from "@/components/ui/input";
import { KnowledgeLibraryViewerDialog } from "@/components/features/knowledge/knowledge-library-viewer-dialog";
import { KnowledgeLibraryCreateDialog } from "@/components/features/knowledge/knowledge-library-create-dialog";
import { KnowledgeOperationDialog } from "@/components/features/knowledge/knowledge-operation-dialog";
import { KnowledgeMessageDialog } from "@/components/features/knowledge/knowledge-feedback";
import { KnowledgePackageImportDialog, KnowledgePackagePickerDialog } from "@/components/features/knowledge/knowledge-package-dialogs";
import { useI18n, type TranslationKey } from "@/i18n";
import { formatKnowledgeIndexStatus } from "@/lib/knowledge-index-status";
import {
  browseKnowledgeSourceDirectories,
  deleteKnowledgeLibrary,
  exportKnowledgePackage,
  getKnowledgeIndexIntegrity,
  importKnowledgePackage,
  inspectKnowledgePackage,
  listKnowledgeLibraries,
  listKnowledgeSources,
  renameKnowledgeLibrary,
} from "@/service/knowledge-base-service";
import type { KnowledgeDisplayIndexState, KnowledgeIndexIntegritySummary, KnowledgeLibrarySummary, KnowledgePackageInspection, KnowledgeSourceSummary } from "@/types";
import type { KnowledgeBusyOperation, KnowledgeSourcePickerTarget } from "./knowledge-area-types";
import { useKnowledgeLibraryPolicyDialogs } from "./use-knowledge-library-policy-dialogs";
import { useKnowledgeOperationJob } from "./use-knowledge-operation-job";
import { formatDateTime } from "@/lib/display-format";

// KnowledgeReadFailure keeps a failed read distinct from empty content and exposes only read recovery.
// KnowledgeReadFailure 区分读取失败与空内容，仅提供读取恢复操作。
// Parameters: error is the exact diagnostic, busy disables recovery, onRetry repeats the owning read.
// 参数：error 是精确诊断，busy 禁用恢复按钮，onRetry 仅重复所属读取。
// Returns a compact localized summary with expandable details and a retry button.
// 返回包含可展开详情与重试按钮的紧凑本地化摘要。
function KnowledgeReadFailure({ error, busy, onRetry }: {
  // Exact transport or service diagnostic, disclosed only on demand.
  // 仅按需展开的精确传输或服务诊断。
  error: string;
  // Whether another operation currently prevents this read retry.
  // 当前是否有其他操作阻止重试读取。
  busy: boolean;
  // Repeats this read without replaying any prior mutation.
  // 仅重复本次读取，不重放此前变更。
  onRetry: () => void;
}) {
  // t provides the same locale as the owning knowledge page.
  // t 与所属知识库页面使用相同语言。
  const { t } = useI18n();
  return <div className="space-y-2 rounded-md border border-destructive/30 p-3 text-sm" role="alert">
    <p>{t("knowledge.readFailed")}</p>
    <details><summary className="cursor-pointer text-muted-foreground">{t("toolPresentation.output.errorDetails")}</summary><pre className="max-h-32 overflow-auto whitespace-pre-wrap break-words text-xs">{error}</pre></details>
    <Button disabled={busy} onClick={onRetry} size="sm" type="button" variant="outline"><RefreshCw size={14} />{t("knowledge.refresh")}</Button>
  </div>;
}

// KnowledgeDisplayBuildAction describes the only action exposed by the display-generation button.
// KnowledgeDisplayBuildAction 描述显示库生成按钮公开的唯一动作。
export type KnowledgeDisplayBuildAction = {
  // Stable action mode used by click handling.
  // 点击处理使用的稳定动作模式。
  mode: "start" | "resume" | "progress" | "complete";
  // User-visible button label matching the persisted lifecycle.
  // 与持久化生命周期匹配的用户可见按钮标签。
  label: TranslationKey;
  // Whether starting another generation is forbidden.
  // 是否禁止再次启动生成。
  disabled: boolean;
};

// knowledgeDisplayBuildAction derives truthful generation behavior from durable display state.
// knowledgeDisplayBuildAction 根据持久化显示库状态派生真实的生成行为。
// Parameters: displayIndex is the authoritative persisted display lifecycle. Returns: one button action.
// 参数：displayIndex 是权威持久化显示库生命周期。返回值：一个按钮动作。
export function knowledgeDisplayBuildAction(displayIndex: KnowledgeDisplayIndexState): KnowledgeDisplayBuildAction {
  if (displayIndex.status === "building") {
    return { mode: "progress", label: "knowledge.experience.viewProgress", disabled: false };
  }
  if (displayIndex.pendingGeneration) {
    return { mode: "resume", label: "knowledge.experience.resume", disabled: false };
  }
  if (displayIndex.skippedDocuments?.some((document) => document.reason === "not_selected")) {
    return { mode: "resume", label: "knowledge.experience.resume", disabled: false };
  }
  if (displayIndex.status === "ready" && displayIndex.activeGeneration) {
    return { mode: "complete", label: "knowledge.experience.displayReady", disabled: true };
  }
  if (displayIndex.status === "stale" || (displayIndex.status === "failed" && displayIndex.activeGeneration)) {
    return { mode: "start", label: "knowledge.experience.regenerate", disabled: false };
  }
  return { mode: "start", label: "knowledge.experience.buildDisplay", disabled: false };
}

// KnowledgeBaseArea renders host-owned library content management and portable operations.
// KnowledgeBaseArea 渲染宿主持有的知识库内容管理与可移植操作。
// Parameters: none; consumes the knowledge service and language context. Returns library management, exact-owner dialogs and durable operation progress.
// 参数：无，消费知识库服务与语言上下文。返回资料库管理、精确归属弹窗及持久操作进度。
export function KnowledgeBaseArea() {
  // t resolves all user-visible knowledge-area copy in the active WebUI language.
  // t 使用当前 WebUI 语言解析知识库区域的全部用户可见文案。
  const { t, locale } = useI18n();
  // libraries stores the authoritative registry rows loaded from the service.
  // libraries 保存从服务加载的权威注册记录。
  const [libraries, setLibraries] = React.useState<KnowledgeLibrarySummary[]>([]);
  // fileReview freezes the exact operation until the user confirms or cancels the inspected list.
  // fileReview 冻结精确操作，直到用户确认或取消已检查清单。
  const [fileReview, setFileReview] = React.useState<KnowledgeFileReviewTarget | null>(null);
  // Library read state distinguishes an initial request, a valid empty registry and a failed refresh.
  // 知识库读取状态区分首次请求、有效空目录与刷新失败。
  const [librariesLoading, setLibrariesLoading] = React.useState(true);
  // librariesError retains the latest read failure until an explicit retry succeeds.
  // librariesError 保留最近读取错误，直到显式重试成功。
  const [librariesError, setLibrariesError] = React.useState<string | null>(null);
  // libraryRequestSequence prevents an older registry response from replacing newer data.
  // libraryRequestSequence 防止较旧的目录响应替换新数据。
  const libraryRequestSequence = React.useRef(0);
  // sources stores the durable Markdown roots attached to the selected library.
  // sources 保存关联到选中知识库的持久化 Markdown 根目录。
  const [sources, setSources] = React.useState<KnowledgeSourceSummary[]>([]);
  // sourcesLoading distinguishes an empty source list from an in-flight management query.
  // sourcesLoading 区分空来源列表与正在进行的管理查询。
  const [sourcesLoading, setSourcesLoading] = React.useState(false);
  // sourcesError is local to the selected library's source query, not an empty directory result.
  // sourcesError 属于选中知识库的来源查询，不代表空目录结果。
  const [sourcesError, setSourcesError] = React.useState<string | null>(null);
  // indexIntegrity stores exact revision and physical-row diagnostics for the selected library.
  // indexIntegrity 保存选中知识库的精确版本与物理记录诊断。
  const [indexIntegrity, setIndexIntegrity] = React.useState<KnowledgeIndexIntegritySummary | null>(null);
  // indexIntegrityLoading distinguishes an unavailable snapshot from an in-flight inspection.
  // indexIntegrityLoading 区分不可用快照与正在进行的检查。
  const [indexIntegrityLoading, setIndexIntegrityLoading] = React.useState(false);
  // indexIntegrityError preserves a failed inspection independently of source-list availability.
  // indexIntegrityError 独立保留检查失败状态，不依赖来源列表是否可用。
  const [indexIntegrityError, setIndexIntegrityError] = React.useState<string | null>(null);
  // sourceRequestSequence rejects stale source-list responses after rapid library selection changes.
  // sourceRequestSequence 在快速切换知识库后拒绝过期来源列表响应。
  const sourceRequestSequence = React.useRef(0);
  // integrityRequestSequence rejects stale integrity responses after rapid selection changes.
  // integrityRequestSequence 在快速切换选择后拒绝过期完整性响应。
  const integrityRequestSequence = React.useRef(0);
  // selectedLibraryId identifies the detail panel target.
  // selectedLibraryId 标识详情面板目标。
  const [selectedLibraryId, setSelectedLibraryId] = React.useState<string | null>(null);
  // deleteTarget freezes the exact library named by the shared destructive confirmation.
  // deleteTarget 冻结共享删除确认明确指定的精确知识库。
  const [deleteTarget, setDeleteTarget] = React.useState<KnowledgeLibrarySummary | null>(null);
  // deleteError remains visible in the confirmation when deletion fails.
  // deleteError 在删除失败时继续显示于确认框内。
  const [deleteError, setDeleteError] = React.useState<string | null>(null);
  // searchText filters the local library summaries.
  // searchText 筛选本地知识库摘要。
  const [searchText, setSearchText] = React.useState("");
  // viewerLibraryId identifies the exact library opened in the dedicated reader dialog.
  // viewerLibraryId 标识在独立阅读弹窗中打开的精确知识库。
  const [viewerLibraryId, setViewerLibraryId] = React.useState<string | null>(null);
  // renameDraft stores editable metadata for the selected library.
  // renameDraft 保存选中知识库的可编辑元数据。
  const [renameDraft, setRenameDraft] = React.useState("");
  // busy stores the exact locale-independent active operation.
  // busy 保存精确且与语言无关的活动操作。
  const [busy, setBusy] = React.useState<KnowledgeBusyOperation | null>(null);
  // busyRef synchronously closes the duplicate-click window before React publishes busy state.
  // busyRef 在 React 发布忙碌状态前同步关闭重复点击窗口。
  const busyRef = React.useRef<KnowledgeBusyOperation | null>(null);
  // actionError owns non-job failures displayed by the shared in-window dialog.
  // actionError 持有由窗口内共享弹窗展示的非任务失败。
  const [actionError, setActionError] = React.useState<string | null>(null);
  // sourcePickerTarget controls the directory browser and preserves its add-or-remap intent.
  // sourcePickerTarget 控制目录浏览器并保留其新增或重新映射意图。
  const [sourcePickerTarget, setSourcePickerTarget] = React.useState<KnowledgeSourcePickerTarget | null>(null);
  // packagePickerOpen controls the host-side .vkb file browser.
  // packagePickerOpen 控制宿主侧 .vkb 文件浏览器。
  const [packagePickerOpen, setPackagePickerOpen] = React.useState(false);
  // packageInspection stores the validated package awaiting an import decision.
  // packageInspection 保存等待导入决策的已校验知识库包。
  const [packageInspection, setPackageInspection] = React.useState<KnowledgePackageInspection | null>(null);
  // exportPickerOpen controls the destination-directory picker.
  // exportPickerOpen 控制导出目标目录选择器。
  const [exportPickerOpen, setExportPickerOpen] = React.useState(false);
  // exportLibraryId identifies the exact row being exported.
  // exportLibraryId 标识正在导出的精确知识库行。
  const [exportLibraryId, setExportLibraryId] = React.useState<string | null>(null);
  // selectedLibrary resolves the immutable current row from the registry.
  // selectedLibrary 从注册表解析不可变当前记录。
  const selectedLibrary = libraries.find((library) => library.id === selectedLibraryId) ?? null;
  // viewerLibrary resolves the immutable library row owned by the open reader.
  // viewerLibrary 解析打开的阅读器所持有的不可变知识库记录。
  const viewerLibrary = libraries.find((library) => library.id === viewerLibraryId) ?? null;
  // exportLibrary resolves the immutable export target selected before directory browsing.
  // exportLibrary 解析在目录浏览前选中的不可变导出目标。
  const exportLibrary = libraries.find((library) => library.id === exportLibraryId) ?? null;
  // sourcePickerSource resolves the exact durable source being remapped, when any.
  // sourcePickerSource 解析正在重新映射的精确持久化来源（如有）。
  const sourcePickerSource = sourcePickerTarget?.sourceId
    ? sources.find((source) => source.id === sourcePickerTarget.sourceId) ?? null
    : null;

  // acquireBusy atomically claims the single page mutation slot for one exact operation.
  // acquireBusy 为一个精确操作原子占用唯一页面变更槽位。
  // Parameters: operation is the stable mutation identity. Returns: whether the slot was acquired.
  // 参数：operation 是稳定的变更身份。返回值：是否成功占用槽位。
  const acquireBusy = React.useCallback((operation: KnowledgeBusyOperation): boolean => {
    if (busyRef.current !== null) return false;
    busyRef.current = operation;
    setBusy(operation);
    return true;
  }, []);

  // releaseBusy releases only the exact operation that currently owns the mutation slot.
  // releaseBusy 仅释放当前持有变更槽位的精确操作。
  // Parameters: operation is the expected current owner. Returns: nothing.
  // 参数：operation 是预期的当前所有者。返回值：无。
  const releaseBusy = React.useCallback((operation: KnowledgeBusyOperation): void => {
    if (busyRef.current !== operation) return;
    busyRef.current = null;
    setBusy(null);
  }, []);

  React.useEffect(() => {
    setRenameDraft(selectedLibrary?.name ?? "");
  }, [selectedLibrary?.id, selectedLibrary?.name]);

  // refreshLibraries reloads the registry and preserves a valid selection.
  // refreshLibraries 重新加载注册表并保留有效选择。
  const refreshLibraries = React.useCallback(async () => {
    // requestSequence owns this registry read even when background reconciliation overlaps refresh.
    // requestSequence 在后台协调与刷新重叠时仍唯一拥有本次目录读取。
    const requestSequence = ++libraryRequestSequence.current;
    setLibrariesLoading(true);
    setLibrariesError(null);
    try {
      // nextLibraries is the authoritative registry, never synthesized from a failed response.
      // nextLibraries 是权威目录，绝不从失败响应构造。
      const nextLibraries = await listKnowledgeLibraries();
      if (requestSequence !== libraryRequestSequence.current) return;
      setLibraries(nextLibraries);
      setSelectedLibraryId((current) => nextLibraries.some((library) => library.id === current) ? current : (nextLibraries[0]?.id ?? null));
    } catch (error) {
      if (requestSequence !== libraryRequestSequence.current) return;
      setLibrariesError(error instanceof Error ? error.message : String(error));
      throw error;
    } finally {
      if (requestSequence === libraryRequestSequence.current) setLibrariesLoading(false);
    }
  }, []);

  // refreshSources reloads one library's source roots and ignores any superseded response.
  // refreshSources 重新加载一个知识库的来源根目录，并忽略任何已被替代的响应。
  // Parameters: libraryId is the exact selected library. Returns: a promise completed after projection.
  // 参数：libraryId 是精确选中的知识库。返回值：投影完成后结束的 Promise。
  const refreshSources = React.useCallback(async (libraryId: string): Promise<void> => {
    // requestSequence is the monotonic identity for this exact request.
    // requestSequence 是本次精确请求的单调身份。
    const requestSequence = ++sourceRequestSequence.current;
    // Clearing the prior projection prevents a failed request from exposing another library's roots.
    // 清空上一份投影，避免请求失败后暴露另一个知识库的根目录。
    setSources([]);
    setSourcesLoading(true);
    setSourcesError(null);
    try {
      // nextSources is the authoritative source snapshot returned by the host.
      // nextSources 是宿主返回的权威来源快照。
      const nextSources = await listKnowledgeSources(libraryId);
      if (requestSequence === sourceRequestSequence.current) setSources(nextSources);
    } catch (error) {
      if (requestSequence !== sourceRequestSequence.current) return;
      setSourcesError(error instanceof Error ? error.message : String(error));
      throw error;
    } finally {
      if (requestSequence === sourceRequestSequence.current) setSourcesLoading(false);
    }
  }, []);

  // refreshIndexIntegrity reloads exact metadata and physical FTS counts for one library.
  // refreshIndexIntegrity 重新加载一个知识库的精确元数据与物理 FTS 数量。
  // Parameters: libraryId is the exact selected library. Returns: a promise completed after projection.
  // 参数：libraryId 是精确选中的知识库。返回值：投影完成后结束的 Promise。
  const refreshIndexIntegrity = React.useCallback(async (libraryId: string): Promise<void> => {
    // requestSequence is the monotonic identity for this exact inspection.
    // requestSequence 是本次精确检查的单调身份。
    const requestSequence = ++integrityRequestSequence.current;
    setIndexIntegrity(null);
    setIndexIntegrityLoading(true);
    setIndexIntegrityError(null);
    try {
      // integrity is computed by the host from authoritative rows and the physical FTS table.
      // integrity 由宿主根据权威记录与物理 FTS 表计算。
      const integrity = await getKnowledgeIndexIntegrity(libraryId);
      if (requestSequence === integrityRequestSequence.current) setIndexIntegrity(integrity);
    } catch (error) {
      if (requestSequence !== integrityRequestSequence.current) return;
      setIndexIntegrityError(error instanceof Error ? error.message : String(error));
      throw error;
    } finally {
      if (requestSequence === integrityRequestSequence.current) setIndexIntegrityLoading(false);
    }
  }, []);

  React.useEffect(() => {
    // Read failures already own an inline recovery surface; no duplicate transient toast is needed.
    // 读取失败已有内联恢复区域，无需再重复弹出临时提示。
    void refreshLibraries().catch(() => undefined);
    return () => { libraryRequestSequence.current += 1; };
  }, [refreshLibraries]);

  React.useEffect(() => {
    if (!selectedLibraryId) {
      sourceRequestSequence.current += 1;
      integrityRequestSequence.current += 1;
      setSources([]);
      setSourcesLoading(false);
      setSourcesError(null);
      setIndexIntegrity(null);
      setIndexIntegrityLoading(false);
      setIndexIntegrityError(null);
      return;
    }
    // Each read renders its own persistent failure and retries only that read.
    // 每项读取独立展示持久错误，重试时仅重读该项。
    void refreshSources(selectedLibraryId).catch(() => undefined);
    void refreshIndexIntegrity(selectedLibraryId).catch(() => undefined);
    return () => {
      sourceRequestSequence.current += 1;
      integrityRequestSequence.current += 1;
    };
  }, [refreshIndexIntegrity, refreshSources, selectedLibraryId]);

  // operationJob owns the durable import, index and derived-build operation and its progress dialog.
  // operationJob 持有持久化导入、索引与派生构建操作及其进度弹窗。
  const operationJob = useKnowledgeOperationJob({
    selectedLibraryId,
    busy,
    acquireBusy,
    releaseBusy,
    refreshLibraries,
    refreshSources,
    refreshIndexIntegrity,
    setFileReview,
    setSourcePickerTarget,
    t,
  });
  const {
    importJob,
    importLibraryId,
    operationRunning,
    startDirectoryImport,
    startIndexOperation,
    startDerivedOperation,
    reopenDerivedOperationProgress,
  } = operationJob;
  // pageBusy prevents authoritative mutations from overlapping a local request or durable operation.
  // pageBusy 防止权威变更与本地请求或持久化操作相互重叠。
  const pageBusy = busy !== null || operationRunning;

  // runMutation serializes page mutations and refreshes authoritative registry state.
  // runMutation 串行化页面变更并刷新权威注册状态。
  const runMutation = React.useCallback(async (operation: KnowledgeBusyOperation, label: string, action: () => Promise<string | void>) => {
    if (!acquireBusy(operation)) return;
    try {
      const successDetail = await action();
      await refreshLibraries();
      toast.success(successDetail ?? t("knowledge.operationCompleted").replace("{operation}", label));
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      releaseBusy(operation);
    }
  }, [acquireBusy, refreshLibraries, releaseBusy, t]);

  // refreshLibraryList reloads registry state with local busy feedback and an in-window error dialog.
  // refreshLibraryList 使用本地忙碌反馈与窗口内错误弹窗重新加载注册状态。
  const refreshLibraryList = React.useCallback(async () => {
    if (!acquireBusy("refresh")) return;
    try {
      await refreshLibraries();
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      releaseBusy("refresh");
    }
  }, [acquireBusy, refreshLibraries, releaseBusy]);

  // policyDialogs owns the create and settings policy drafts shared by both library dialogs.
  // policyDialogs 持有两个知识库弹窗共用的创建与设置策略草稿。
  const {
    createName,
    setCreateName,
    createDialogOpen,
    setCreateDialogOpen,
    settingsDialogOpen,
    setSettingsDialogOpen,
    policyDialogFields,
    openCreateDialog,
    openSettingsDialog,
    createLibrary,
    updateLibrarySettings,
  } = useKnowledgeLibraryPolicyDialogs({
    selectedLibrary,
    acquireBusy,
    releaseBusy,
    runMutation,
    refreshLibraries,
    setActionError,
    setSelectedLibraryId,
    t,
  });

  // exportPackage writes one consistent package to the selected host directory.
  // exportPackage 将一个一致性知识库包写入选中的宿主目录。
  const exportPackage = React.useCallback(async (path: string) => {
    if (!exportLibrary || !acquireBusy("package-export")) return;
    try {
      const result = await exportKnowledgePackage(exportLibrary.id, path);
      toast.success(t("knowledge.package.exportCompleted").replace("{path}", result.archivePath));
    } finally {
      releaseBusy("package-export");
    }
  }, [acquireBusy, exportLibrary, releaseBusy, t]);

  // inspectPackage validates one selected archive without mutating any library.
  // inspectPackage 校验一个选中归档且不修改任何知识库。
  const inspectPackage = React.useCallback(async (path: string) => {
    const inspection = await inspectKnowledgePackage(path);
    setPackageInspection(inspection);
  }, []);

  // executePackageImport creates or explicitly replaces one library from the validated archive.
  // executePackageImport 从已校验归档新建或显式替换一个知识库。
  const executePackageImport = React.useCallback(async (name: string, strategy: "create" | "replace", targetLibraryId: string | null) => {
    if (!packageInspection) return;
    // operation distinguishes destructive replacement from a new independent import.
    // operation 区分破坏性覆盖与新的独立导入。
    const operation = strategy === "replace" ? "package-replace" : "package-import";
    if (!acquireBusy(operation)) return;
    try {
      const result = await importKnowledgePackage(packageInspection.archivePath, name, strategy, targetLibraryId);
      await refreshLibraries();
      setSelectedLibraryId(result.library.id);
      setPackageInspection(null);
      toast.success(result.replacedExisting ? t("knowledge.package.replaceCompleted") : t("knowledge.package.importCompleted"));
      if (result.warnings.length > 0) setActionError(result.warnings.join("\n"));
    } finally {
      releaseBusy(operation);
    }
  }, [acquireBusy, packageInspection, refreshLibraries, releaseBusy, t]);

  // filteredLibraries applies the local quick filter without changing service state.
  // filteredLibraries 应用本地快速筛选且不改变服务状态。
  const filteredLibraries = libraries.filter((library) => `${library.name} ${library.description}`.toLocaleLowerCase().includes(searchText.toLocaleLowerCase()));
  // displayBuildAction projects the selected library's durable generation state into one truthful button.
  // displayBuildAction 将选中资料库的持久化生成状态投影为一个真实按钮。
  const displayBuildAction = selectedLibrary
    ? (operationRunning
      && importLibraryId === selectedLibrary.id
      && importJob?.operationKind === "display_build"
      ? { mode: "progress", label: "knowledge.experience.viewProgress", disabled: false } satisfies KnowledgeDisplayBuildAction
      : knowledgeDisplayBuildAction(selectedLibrary.settings.displayIndex))
    : null;
  // handleViewerOpenChange clears the exact reader owner when the dialog closes.
  // handleViewerOpenChange 在弹窗关闭时清除精确的阅读器所有者。
  const handleViewerOpenChange = React.useCallback((open: boolean) => {
    if (!open) setViewerLibraryId(null);
  }, []);
  return (
    <section className="knowledge-page">
      <div className="knowledge-toolbar">
        <div className="knowledge-create-row">
          <Input aria-label={t("knowledge.name")} disabled={pageBusy} onChange={(event) => setCreateName(event.target.value)} placeholder={t("knowledge.name.placeholder")} value={createName} />
          <Button disabled={pageBusy || !createName.trim()} onClick={() => void openCreateDialog()}><Plus size={15} />{t("knowledge.create")}</Button>
        </div>
        <label className="knowledge-search"><Search size={15} /><Input aria-label={t("knowledge.filter.aria")} onChange={(event) => setSearchText(event.target.value)} placeholder={t("knowledge.filter.placeholder")} value={searchText} /></label>
        <Button disabled={pageBusy} onClick={() => setPackagePickerOpen(true)} variant="outline"><Upload size={15} />{t("knowledge.package.import")}</Button>
        <Button disabled={pageBusy} onClick={() => void refreshLibraryList()} variant="outline"><RefreshCw className={busy === "refresh" ? "animate-spin" : undefined} size={15} />{t("knowledge.refresh")}</Button>
      </div>
      <div className="knowledge-layout">
        <aside className="knowledge-library-list">
          {librariesLoading ? <StreamPlaceholder fill label={t("knowledge.librariesLoading")} shape="rows" count={4} /> : null}
          {librariesError ? <KnowledgeReadFailure busy={pageBusy || librariesLoading} error={librariesError} onRetry={() => void refreshLibraryList()} /> : null}
          {filteredLibraries.map((library) => (
            <div className={library.id === selectedLibraryId ? "knowledge-library-row active" : "knowledge-library-row"} key={library.id}>
              <button className="knowledge-library-select" disabled={pageBusy} onClick={() => setSelectedLibraryId(library.id)} type="button">
                <Database size={18} />
                <span><strong>{library.name}</strong><small title={library.indexStatus}>{t("knowledge.libraryCounts").replace("{documents}", String(library.documentCount)).replace("{chunks}", String(library.chunkCount)).replace("{status}", formatKnowledgeIndexStatus(library.indexStatus, t))}</small></span>
              </button>
              <Button aria-label={`${t("knowledge.view")} ${library.name}`} className="knowledge-library-view-button" disabled={pageBusy} onClick={() => {
                setSelectedLibraryId(library.id);
                setViewerLibraryId(library.id);
              }} size="icon-sm" title={`${t("knowledge.view")} ${library.name}`} type="button" variant="ghost"><BookOpen size={16} /></Button>
              <Button aria-label={`${t("knowledge.package.export")} ${library.name}`} className="knowledge-library-view-button" disabled={pageBusy} onClick={() => {
                setSelectedLibraryId(library.id);
                setExportLibraryId(library.id);
                setExportPickerOpen(true);
              }} size="icon-sm" title={`${t("knowledge.package.export")} ${library.name}`} type="button" variant="ghost"><Download size={16} /></Button>
            </div>
          ))}
          {!librariesLoading && !librariesError && filteredLibraries.length === 0 ? <div className="knowledge-empty">{t(libraries.length > 0 ? "knowledge.noMatchingLibraries" : "knowledge.emptyLibraries")}</div> : null}
        </aside>
        <main className="knowledge-detail">
          {selectedLibrary ? (
            <>
              <header className="knowledge-detail-header">
                <div className="knowledge-title-editor"><Input aria-label={t("knowledge.name")} disabled={pageBusy} onChange={(event) => setRenameDraft(event.target.value)} value={renameDraft} /><p>{selectedLibrary.description || t("knowledge.defaultDescription")}</p></div>
                <div className="knowledge-actions">
                  <Button disabled={pageBusy} onClick={() => void openSettingsDialog()} variant="outline"><Settings2 size={15} />{t("knowledge.experience.configure")}</Button>
                  <Button disabled={pageBusy} onClick={() => setFileReview({ kind: "ignored", libraryId: selectedLibrary.id })} variant="outline">{t("knowledge.files.ignored")} ({selectedLibrary.settings.ignoredFiles?.length ?? 0})</Button>
                  <Button disabled={pageBusy || !renameDraft.trim() || renameDraft.trim() === selectedLibrary.name} onClick={() => void runMutation("rename", t("knowledge.saveName"), async () => {
                    await renameKnowledgeLibrary(selectedLibrary.id, renameDraft.trim(), selectedLibrary.description);
                  })} variant="outline">{t("knowledge.saveName")}</Button>
                  <Button disabled={pageBusy} onClick={() => setSourcePickerTarget({ sourceId: null })}><FolderInput size={15} />{t("knowledge.sources.add")}</Button>
                  <Button disabled={pageBusy} onClick={() => { setDeleteTarget(selectedLibrary); setDeleteError(null); }} variant="destructive"><Trash2 size={15} />{t("knowledge.delete")}</Button>
                </div>
              </header>
              <section className="knowledge-index-health">
                <header className="knowledge-index-health-header">
                  <div>
                    <h3>{t("knowledge.indexHealth.title")}</h3>
                    <p>{t("knowledge.indexHealth.description")}</p>
                  </div>
                  {indexIntegrity ? (
                    <span className={`knowledge-index-health-state ${indexIntegrity.searchable ? "ready" : indexIntegrity.indexStatus === "empty" ? "" : "attention"}`}>
                      {indexIntegrity.searchable ? t("knowledge.indexHealth.searchable") : indexIntegrity.indexStatus === "empty" ? t("knowledge.indexStatus.empty") : t("knowledge.indexHealth.needsRepair")}
                    </span>
                  ) : null}
                </header>
                {indexIntegrityLoading ? <StreamPlaceholder label={t("knowledge.indexHealth.loading")} shape="tiles" count={3} /> : null}
                {indexIntegrityError ? <KnowledgeReadFailure busy={pageBusy || indexIntegrityLoading} error={indexIntegrityError} onRetry={() => void refreshIndexIntegrity(selectedLibrary.id).catch(() => undefined)} /> : null}
                {!indexIntegrityLoading && indexIntegrity ? (
                  <>
                    <div className="knowledge-index-health-metrics">
                      <span><strong>{indexIntegrity.authoritativeChunkCount}</strong><small>{t("knowledge.indexHealth.authoritative")}</small></span>
                      <span><strong>{indexIntegrity.indexedChunkCount}</strong><small>{t("knowledge.indexHealth.indexed")}</small></span>
                      <span><strong>{indexIntegrity.missingOrMismatchedCount}</strong><small>{t("knowledge.indexHealth.missing")}</small></span>
                      <span><strong>{indexIntegrity.orphanOrMismatchedCount}</strong><small>{t("knowledge.indexHealth.orphaned")}</small></span>
                    </div>
                    <div className="knowledge-index-health-revisions">
                      <code>{t("knowledge.indexHealth.contentRevision").replace("{indexed}", String(indexIntegrity.indexedContentRevision)).replace("{current}", String(indexIntegrity.contentRevision))}</code>
                      <code>{t("knowledge.indexHealth.dictionaryRevision").replace("{indexed}", String(indexIntegrity.indexedDictionaryRevision)).replace("{current}", String(indexIntegrity.dictionaryRevision))}</code>
                      <code>{t("knowledge.indexHealth.tokenizer").replace("{tokenizer}", indexIntegrity.tokenizer || "-")}</code>
                    </div>
                    {selectedLibrary.settings.vectorIndex.compatibilityOverride ? <p className="text-sm text-[var(--vcn-content-warning)]">{t("knowledge.experience.compatibilityActive").replace("{reason}", selectedLibrary.settings.vectorIndex.compatibilityOverride)}</p> : null}
                    <div className="knowledge-index-health-actions">
                      <Button disabled={pageBusy || indexIntegrity.searchable} onClick={() => void startIndexOperation("repair", selectedLibrary.id)} variant="outline"><Wrench size={15} />{t("knowledge.repairIndex")}</Button>
                      <Button disabled={pageBusy} onClick={() => void startIndexOperation("rebuild", selectedLibrary.id)} variant="outline"><RefreshCw size={15} />{t("knowledge.rebuildIndex")}</Button>
                      {selectedLibrary.settings.curation.enabled ? (
                        <Button
                          disabled={Boolean(displayBuildAction?.disabled) || (pageBusy && displayBuildAction?.mode !== "progress")}
                          onClick={() => {
                            if (displayBuildAction?.mode === "progress") {
                              reopenDerivedOperationProgress(selectedLibrary.id);
                            } else {
                              void startDerivedOperation("display-build", selectedLibrary.id);
                            }
                          }}
                          variant="outline"
                        ><Wrench size={15} />{t(displayBuildAction?.label ?? "knowledge.experience.buildDisplay")}</Button>
                      ) : null}
                      {selectedLibrary.settings.embedding.enabled ? (
                        <Button disabled={pageBusy || selectedLibrary.settings.displayIndex.status !== "ready" || selectedLibrary.settings.displayIndex.generatedDocumentCount === 0 || selectedLibrary.settings.vectorIndex.status === "building"} onClick={() => void startDerivedOperation("vector-build", selectedLibrary.id)} variant="outline"><Database size={15} />{t("knowledge.experience.buildVectors")}</Button>
                      ) : null}
                    </div>
                  </>
                ) : null}
              </section>
              {selectedLibrary.settings.curation.enabled ? <>
                <p className="text-sm text-muted-foreground">{t("knowledge.skipped.policy")}</p>
                <KnowledgeSkippedDocuments documents={selectedLibrary.settings.displayIndex.skippedDocuments} />
              </> : null}
              <section className="knowledge-sources">
                <header className="knowledge-sources-header">
                  <div>
                    <h3>{t("knowledge.sources.title")}</h3>
                    <p>{t("knowledge.sources.description")}</p>
                  </div>
                </header>
                {sourcesLoading ? <StreamPlaceholder label={t("knowledge.sources.loading")} shape="rows" count={3} /> : null}
                {sourcesError ? <KnowledgeReadFailure busy={pageBusy || sourcesLoading} error={sourcesError} onRetry={() => void refreshSources(selectedLibrary.id).catch(() => undefined)} /> : null}
                {!sourcesLoading && !sourcesError && sources.length === 0 ? <div className="knowledge-sources-state">{t("knowledge.sources.empty")}</div> : null}
                {!sourcesLoading ? (
                  <div className="knowledge-source-list">
                    {sources.map((source) => (
                      <article className="knowledge-source-row" key={source.id}>
                        <div className="knowledge-source-identity">
                          <FolderInput size={18} />
                          <span>
                            <strong>{source.displayName}</strong>
                            <code title={source.canonicalRoot}>{source.canonicalRoot}</code>
                            <small>
                              {t("knowledge.sources.counts").replace("{documents}", String(source.documentCount)).replace("{chunks}", String(source.chunkCount))}
                              {" · "}
                              {t("knowledge.sources.lastScanned").replace("{time}", formatDateTime(source.lastScannedAtMs, locale) ?? "-")}
                            </small>
                          </span>
                        </div>
                        <div className="knowledge-source-actions">
                          <Button
                            aria-label={`${t("knowledge.sources.sync")} ${source.displayName}`}
                            disabled={pageBusy}
                            onClick={() => startDirectoryImport(selectedLibrary.id, source.canonicalRoot, source.id)}
                            size="icon-sm"
                            title={`${t("knowledge.sources.sync")} ${source.displayName}`}
                            type="button"
                            variant="ghost"
                          ><RefreshCw size={16} /></Button>
                          <Button
                            aria-label={`${t("knowledge.sources.modify")} ${source.displayName}`}
                            disabled={pageBusy}
                            onClick={() => setSourcePickerTarget({ sourceId: source.id })}
                            size="icon-sm"
                            title={`${t("knowledge.sources.modify")} ${source.displayName}`}
                            type="button"
                            variant="ghost"
                          ><FolderPen size={16} /></Button>
                        </div>
                      </article>
                    ))}
                  </div>
                ) : null}
              </section>
            </>
          ) : librariesLoading ? <StreamPlaceholder className="p-4" decorative fill /> : <div className="knowledge-empty">{t("knowledge.emptySelection")}</div>}
        </main>
      </div>
      <AlertDialog open={deleteTarget !== null} onOpenChange={(open) => { if (!open && busyRef.current === null) setDeleteTarget(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("knowledge.delete")}</AlertDialogTitle>
            <AlertDialogDescription>{t("knowledge.deleteConfirm").replace("{name}", deleteTarget?.name ?? "")}</AlertDialogDescription>
          </AlertDialogHeader>
          {deleteError ? <p role="alert" className="text-sm text-destructive">{deleteError}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy !== null}>{t("conversation.action.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={busy !== null} variant="destructive" onClick={(event) => {
              event.preventDefault();
              if (!deleteTarget || !acquireBusy("delete")) return;
              setDeleteError(null);
              void deleteKnowledgeLibrary(deleteTarget.id).then(async () => {
                setDeleteTarget(null);
                await refreshLibraries();
              }).catch((reason) => setDeleteError(reason instanceof Error ? reason.message : String(reason)))
                .finally(() => releaseBusy("delete"));
            }}>{t("knowledge.delete")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <KnowledgeLibraryCreateDialog
        {...policyDialogFields}
        busy={busy === "create"}
        onOpenChange={(open) => { if (busyRef.current === null) setCreateDialogOpen(open) }}
        onSubmit={(model) => void createLibrary(model)}
        open={createDialogOpen}
      />
      <KnowledgeLibraryCreateDialog
        {...policyDialogFields}
        busy={busy === "settings"}
        onOpenChange={(open) => { if (busyRef.current === null) setSettingsDialogOpen(open) }}
        onSubmit={(model) => void updateLibrarySettings(model)}
        open={settingsDialogOpen}
        submitLabel={t("memory.common.save")}
        title={t("knowledge.experience.settingsTitle")}
      />
      {fileReview ? <KnowledgeFileConfirmationDialog
        key={JSON.stringify(fileReview)} target={fileReview}
        ignoredFiles={libraries.find((library) => library.id === fileReview.libraryId)?.settings.ignoredFiles ?? []}
        onClose={() => setFileReview(null)} onRulesChanged={refreshLibraries}
        onConfirm={(confirmation) => {
          setFileReview(null);
          if (fileReview.kind === "import") void startDirectoryImport(fileReview.libraryId, fileReview.sourceRoot, fileReview.sourceId, confirmation);
          else if (fileReview.kind === "display") void startDerivedOperation("display-build", fileReview.libraryId, confirmation);
        }}
      /> : null}
      <KnowledgeLibraryViewerDialog library={viewerLibrary} onOpenChange={handleViewerOpenChange} open={viewerLibrary !== null} />
      <KnowledgePackagePickerDialog onOpenChange={setPackagePickerOpen} onSelect={inspectPackage} open={packagePickerOpen} />
      <KnowledgePackageImportDialog
        busy={busy === "package-import" || busy === "package-replace"}
        inspection={packageInspection}
        libraries={libraries}
        onImport={executePackageImport}
        onOpenChange={(open) => {
          if (!open) setPackageInspection(null);
        }}
      />
      <ProjectFolderPickerDialog
        confirmLabel={t("knowledge.package.export")}
        onBrowse={browseKnowledgeSourceDirectories}
        onConfirm={exportPackage}
        onOpenChange={(open) => {
          setExportPickerOpen(open);
          if (!open && busy !== "package-export") setExportLibraryId(null);
        }}
        open={exportPickerOpen && exportLibrary !== null}
        title={t("knowledge.package.exportPickerTitle")}
      />
      <ProjectFolderPickerDialog
        confirmLabel={t("knowledge.importConfirm")}
        initialPath={sourcePickerSource?.canonicalRoot ?? null}
        onBrowse={browseKnowledgeSourceDirectories}
        onConfirm={async (path) => {
          if (!selectedLibrary || !sourcePickerTarget) {
            return;
          }
          await startDirectoryImport(selectedLibrary.id, path, sourcePickerTarget.sourceId);
        }}
        onOpenChange={(open) => {
          if (!open) setSourcePickerTarget(null);
        }}
        open={sourcePickerTarget !== null}
        title={sourcePickerSource
          ? t("knowledge.sources.modifyPickerTitle").replace("{name}", sourcePickerSource.displayName)
          : t("knowledge.pickerTitle")}
      />
      <KnowledgeOperationDialog operation={operationJob} pageBusy={pageBusy} />
      <KnowledgeMessageDialog message={actionError} onClose={() => setActionError(null)} />
    </section>
  );
}
