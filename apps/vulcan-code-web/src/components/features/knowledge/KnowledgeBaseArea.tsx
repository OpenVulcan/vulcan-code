import * as React from "react";
import { BookOpen, Database, Download, FolderInput, FolderPen, Plus, RefreshCw, Search, Settings2, Trash2, Upload, Wrench } from "lucide-react";
import { toast } from "sonner";

import { ProjectFolderPickerDialog } from "@/components/project/folder-picker-dialog";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { KnowledgeLibraryViewerDialog } from "@/components/features/knowledge/KnowledgeLibraryViewerDialog";
import { KnowledgeLibraryCreateDialog, type KnowledgeLibraryCreateDialogModel } from "@/components/features/knowledge/KnowledgeLibraryCreateDialog";
import { KnowledgeOperationProgress } from "@/components/features/knowledge/KnowledgeOperationProgress";
import { KnowledgePackageImportDialog, KnowledgePackagePickerDialog } from "@/components/features/knowledge/KnowledgePackageDialogs";
import { useI18n } from "@/i18n";
import { loadPublicAiSettings } from "@/service/settings-actions-service";
import {
  browseKnowledgeSourceDirectories,
  buildKnowledgeDisplayIndex,
  buildKnowledgeVectorIndex,
  cancelKnowledgeImport,
  createKnowledgeLibrary,
  deleteKnowledgeLibrary,
  exportKnowledgePackage,
  getKnowledgeIndexIntegrity,
  getKnowledgeImportJob,
  importKnowledgeDirectory,
  importKnowledgePackage,
  inspectKnowledgePackage,
  listKnowledgeLibraries,
  listKnowledgeSources,
  renameKnowledgeLibrary,
  updateKnowledgeLibrarySettings,
  repairKnowledgeIndex,
  rebuildKnowledgeIndex,
} from "@/service/knowledge-base-service";
import type { KnowledgeDisplayIndexState, KnowledgeImportJobSummary, KnowledgeIndexIntegritySummary, KnowledgeLibrarySummary, KnowledgePackageInspection, KnowledgeRetrievalMode, KnowledgeSourceSummary } from "@/types";
import type { PublicAiInternalInferenceCandidate } from "@/types/public-ai";

// KnowledgeBusyOperation is the locale-independent identity of one serialized page mutation.
// KnowledgeBusyOperation 是一次串行页面变更的语言无关身份。
type KnowledgeBusyOperation =
  | "create"
  | "settings"
  | "rename"
  | "delete"
  | "repair"
  | "rebuild"
  | "display-build"
  | "vector-build"
  | "refresh"
  | "directory-import"
  | "package-export"
  | "package-import"
  | "package-replace";

// KnowledgeSourcePickerTarget identifies whether directory browsing adds a source or remaps one.
// KnowledgeSourcePickerTarget 标识目录浏览是在新增来源还是重新映射现有来源。
type KnowledgeSourcePickerTarget = {
  // Existing source identity to remap, or null when adding or synchronizing by path.
  // 要重新映射的现有来源身份；按路径新增或同步时为空。
  sourceId: string | null;
};

// knowledgeCandidateRouteKey derives the same immutable route key as the creation dialog.
// knowledgeCandidateRouteKey 根据不可变路由生成与创建对话框一致的键。
function knowledgeCandidateRouteKey(candidate: PublicAiInternalInferenceCandidate): string {
  const route = candidate.route;
  return [route.provider_id, route.service_entry_id, route.connection_id, route.model_id, route.operation, route.binding.kind, route.binding.kind === "protocol" ? route.binding.protocol_profile_id : route.binding.action_binding_id].join("/");
}

// knowledgeImportProgressPercent returns a bounded determinate percentage after scanning completes.
// knowledgeImportProgressPercent 在扫描完成后返回有界的确定进度百分比。
export function knowledgeImportProgressPercent(job: KnowledgeImportJobSummary | null): number | null {
  if (!job) return null;
  if (job.phase === "indexing" || job.phase === "repairing" || job.phase === "rebuilding" || job.phase === "curating" || job.phase === "embedding") {
    if (job.totalIndexItems <= 0) return null;
    return Math.min(100, Math.max(0, Math.round((job.processedIndexItems / job.totalIndexItems) * 100)));
  }
  if (job.operationKind !== "import" || job.totalFiles <= 0) return null;
  return Math.min(100, Math.max(0, Math.round((job.processedFiles / job.totalFiles) * 100)));
}

// knowledgeImportHasCommittedContent identifies import phases reached after authoritative commit.
// knowledgeImportHasCommittedContent 标识在权威内容提交后才会到达的导入阶段。
// Parameters: job is one durable operation snapshot. Returns: whether document and chunk projections can refresh.
// 参数：job 是一份持久化操作快照。返回值：是否可以刷新文档与切片投影。
export function knowledgeImportHasCommittedContent(job: KnowledgeImportJobSummary): boolean {
  return job.operationKind === "import"
    && ["indexing", "rebuilding", "verifying", "finalizing"].includes(job.phase);
}

// KnowledgeDisplayBuildAction describes the only action exposed by the display-generation button.
// KnowledgeDisplayBuildAction 描述显示库生成按钮公开的唯一动作。
export type KnowledgeDisplayBuildAction = {
  // Stable action mode used by click handling.
  // 点击处理使用的稳定动作模式。
  mode: "start" | "resume" | "progress" | "complete";
  // User-visible button label matching the persisted lifecycle.
  // 与持久化生命周期匹配的用户可见按钮标签。
  label: string;
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
    return { mode: "progress", label: "查看生成进度", disabled: false };
  }
  if (displayIndex.pendingGeneration) {
    return { mode: "resume", label: "继续生成", disabled: false };
  }
  if (displayIndex.status === "ready" && displayIndex.activeGeneration) {
    return { mode: "complete", label: "显示库已完成", disabled: true };
  }
  if (displayIndex.status === "stale" || (displayIndex.status === "failed" && displayIndex.activeGeneration)) {
    return { mode: "start", label: "重新生成", disabled: false };
  }
  return { mode: "start", label: "生成显示库", disabled: false };
}

// KnowledgeBaseArea renders host-owned library content management and portable operations.
// KnowledgeBaseArea 渲染宿主持有的知识库内容管理与可移植操作。
export function KnowledgeBaseArea() {
  // t resolves all user-visible knowledge-area copy in the active WebUI language.
  // t 使用当前 WebUI 语言解析知识库区域的全部用户可见文案。
  const { t } = useI18n();
  // libraries stores the authoritative registry rows loaded from the service.
  // libraries 保存从服务加载的权威注册记录。
  const [libraries, setLibraries] = React.useState<KnowledgeLibrarySummary[]>([]);
  // sources stores the durable Markdown roots attached to the selected library.
  // sources 保存关联到选中知识库的持久化 Markdown 根目录。
  const [sources, setSources] = React.useState<KnowledgeSourceSummary[]>([]);
  // sourcesLoading distinguishes an empty source list from an in-flight management query.
  // sourcesLoading 区分空来源列表与正在进行的管理查询。
  const [sourcesLoading, setSourcesLoading] = React.useState(false);
  // indexIntegrity stores exact revision and physical-row diagnostics for the selected library.
  // indexIntegrity 保存选中知识库的精确版本与物理记录诊断。
  const [indexIntegrity, setIndexIntegrity] = React.useState<KnowledgeIndexIntegritySummary | null>(null);
  // indexIntegrityLoading distinguishes an unavailable snapshot from an in-flight inspection.
  // indexIntegrityLoading 区分不可用快照与正在进行的检查。
  const [indexIntegrityLoading, setIndexIntegrityLoading] = React.useState(false);
  // sourceRequestSequence rejects stale source-list responses after rapid library selection changes.
  // sourceRequestSequence 在快速切换知识库后拒绝过期来源列表响应。
  const sourceRequestSequence = React.useRef(0);
  // integrityRequestSequence rejects stale integrity responses after rapid selection changes.
  // integrityRequestSequence 在快速切换选择后拒绝过期完整性响应。
  const integrityRequestSequence = React.useRef(0);
  // selectedLibraryId identifies the detail panel target.
  // selectedLibraryId 标识详情面板目标。
  const [selectedLibraryId, setSelectedLibraryId] = React.useState<string | null>(null);
  // createName stores the new-library name field.
  // createName 保存新知识库名称字段。
  const [createName, setCreateName] = React.useState("");
  // createDialogOpen controls the creation-time policy wizard.
  // createDialogOpen 控制创建阶段的策略向导。
  const [createDialogOpen, setCreateDialogOpen] = React.useState(false);
  // createRetrievalMode stores the selected corpus policy before submission.
  // createRetrievalMode 保存提交前选择的内容层策略。
  const [createRetrievalMode, setCreateRetrievalMode] = React.useState<KnowledgeRetrievalMode>("raw_bm25");
  // createInferenceCandidates mirrors the official capability snapshot used by the wizard.
  // createInferenceCandidates 镜像向导使用的官方能力快照候选。
  const [createInferenceCandidates, setCreateInferenceCandidates] = React.useState<PublicAiInternalInferenceCandidate[]>([]);
  // createCurationCandidateKey identifies the selected LLM route.
  // createCurationCandidateKey 标识所选整理 LLM 路由。
  const [createCurationCandidateKey, setCreateCurationCandidateKey] = React.useState("");
  // createCurationReasoningEffort stores the selected official reasoning level.
  // createCurationReasoningEffort 保存官方声明的整理推理级别。
  const [createCurationReasoningEffort, setCreateCurationReasoningEffort] = React.useState("");
  // createEmbeddingCandidateKey identifies the selected embedding route.
  // createEmbeddingCandidateKey 标识所选向量路由。
  const [createEmbeddingCandidateKey, setCreateEmbeddingCandidateKey] = React.useState("");
  // createConfiguredDimension stores the selected capability dimension.
  // createConfiguredDimension 保存所选能力维度。
  const [createConfiguredDimension, setCreateConfiguredDimension] = React.useState("");
  // createRequestedBatchSize stores an optional user override.
  // createRequestedBatchSize 保存可选的用户批量覆盖值。
  const [createRequestedBatchSize, setCreateRequestedBatchSize] = React.useState("");
  // createCompatibilityOverride stores a deliberate same-dimension index reuse note.
  // createCompatibilityOverride 保存明确复用同维度索引的风险说明。
  const [createCompatibilityOverride, setCreateCompatibilityOverride] = React.useState("");
  // settingsDialogOpen controls editing of the selected library policy.
  // settingsDialogOpen 控制当前资料库策略编辑对话框。
  const [settingsDialogOpen, setSettingsDialogOpen] = React.useState(false);
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
  // importJob stores dedicated progress while a long-running import remains active.
  // importJob 保存长时间运行导入期间的专用进度。
  const [importJob, setImportJob] = React.useState<KnowledgeImportJobSummary | null>(null);
  // importLibraryId keeps progress and cancellation bound to the library that started the job.
  // importLibraryId 将进度和取消操作绑定到启动任务的知识库。
  const [importLibraryId, setImportLibraryId] = React.useState<string | null>(null);
  // importProgressOpen controls the dedicated non-blocking import progress dialog.
  // importProgressOpen 控制独立的非阻塞导入进度对话框。
  const [importProgressOpen, setImportProgressOpen] = React.useState(false);
  // importError stores the terminal failure inside the progress dialog.
  // importError 在进度对话框内保存终态失败信息。
  const [importError, setImportError] = React.useState("");
  // importOutcome preserves the authoritative command result even if final progress refresh fails.
  // importOutcome 即使最终进度刷新失败也会保留权威命令结果。
  const [importOutcome, setImportOutcome] = React.useState<"completed" | "cancelled" | "failed" | null>(null);
  // importCancelRequested gives immediate feedback before the backend reaches a cancellation checkpoint.
  // importCancelRequested 在后端到达取消检查点之前提供即时反馈。
  const [importCancelRequested, setImportCancelRequested] = React.useState(false);
  // importWarnings stores bounded, copyable warnings from the latest completed import.
  // importWarnings 保存最近一次完成导入产生的有界、可复制告警。
  const [importWarnings, setImportWarnings] = React.useState<string[]>([]);
  // operationPollRevision restarts durable polling after a new detached operation is accepted.
  // operationPollRevision 在新的分离式操作被接受后重启持久化轮询。
  const [operationPollRevision, setOperationPollRevision] = React.useState(0);
  // trackedOperationJobId binds terminal handling to the exact operation observed as active.
  // trackedOperationJobId 将终态处理绑定到被观察为活动状态的精确操作。
  const trackedOperationJobId = React.useRef<string | null>(null);
  // handledTerminalJobId prevents duplicate registry refreshes for one persisted terminal row.
  // handledTerminalJobId 防止针对同一持久化终态记录重复刷新注册表。
  const handledTerminalJobId = React.useRef<string | null>(null);
  // projectedAuthoritativeJobId prevents duplicate count refreshes after one import commits content.
  // projectedAuthoritativeJobId 防止一次导入提交内容后重复刷新权威计数。
  const projectedAuthoritativeJobId = React.useRef<string | null>(null);
  // dismissedOperationDialogJobId prevents polling from reopening a job the user deliberately sent to background.
  // dismissedOperationDialogJobId 防止轮询重新打开用户明确转入后台的任务。
  const dismissedOperationDialogJobId = React.useRef<string | null>(null);
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
  // operationRunning is derived from durable backend state rather than the initiating HTTP request.
  // operationRunning 从持久化后端状态派生，而不是依赖发起操作的 HTTP 请求。
  const operationRunning = importJob?.state === "queued" || importJob?.state === "running";
  // pageBusy prevents authoritative mutations from overlapping a local request or durable operation.
  // pageBusy 防止权威变更与本地请求或持久化操作相互重叠。
  const pageBusy = busy !== null || operationRunning;
  // operationDialogRunning includes the short service-start window before the durable job is returned.
  // operationDialogRunning 包含持久化任务返回前的短暂服务启动窗口。
  const operationDialogRunning = operationRunning || busy === "directory-import" || busy === "repair" || busy === "rebuild" || busy === "display-build" || busy === "vector-build";
  // displayedOperationKind preserves a truthful dialog title while a start request is in flight.
  // displayedOperationKind 在启动请求进行期间保持真实的对话框标题。
  const displayedOperationKind = importJob?.operationKind ?? (busy === "repair" ? "repair" : (busy === "rebuild" ? "rebuild" : (busy === "display-build" ? "display_build" : (busy === "vector-build" ? "vector_build" : "import"))));

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
    const nextLibraries = await listKnowledgeLibraries();
    setLibraries(nextLibraries);
    setSelectedLibraryId((current) => nextLibraries.some((library) => library.id === current) ? current : (nextLibraries[0]?.id ?? null));
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
    try {
      // nextSources is the authoritative source snapshot returned by the host.
      // nextSources 是宿主返回的权威来源快照。
      const nextSources = await listKnowledgeSources(libraryId);
      if (requestSequence === sourceRequestSequence.current) setSources(nextSources);
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
    try {
      // integrity is computed by the host from authoritative rows and the physical FTS table.
      // integrity 由宿主根据权威记录与物理 FTS 表计算。
      const integrity = await getKnowledgeIndexIntegrity(libraryId);
      if (requestSequence === integrityRequestSequence.current) setIndexIntegrity(integrity);
    } finally {
      if (requestSequence === integrityRequestSequence.current) setIndexIntegrityLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void refreshLibraries().catch((error) => toast.error(error instanceof Error ? error.message : String(error)));
  }, [refreshLibraries]);

  React.useEffect(() => {
    if (!selectedLibraryId) {
      sourceRequestSequence.current += 1;
      integrityRequestSequence.current += 1;
      setSources([]);
      setSourcesLoading(false);
      setIndexIntegrity(null);
      setIndexIntegrityLoading(false);
      return;
    }
    void refreshSources(selectedLibraryId).catch((error) => toast.error(error instanceof Error ? error.message : String(error)));
    void refreshIndexIntegrity(selectedLibraryId).catch((error) => toast.error(error instanceof Error ? error.message : String(error)));
  }, [refreshIndexIntegrity, refreshSources, selectedLibraryId]);

  React.useEffect(() => {
    if (!selectedLibraryId) return;
    // disposed prevents an obsolete selection from projecting job state into the current library.
    // disposed 防止过期选择将任务状态投影到当前知识库。
    let disposed = false;
    // timer owns recursive polling only while the exact persisted operation remains active.
    // timer 仅在精确持久化操作保持活动时持有递归轮询。
    let timer: number | null = null;
    // poll reads durable state so a WebUI refresh can reconnect without owning the executor future.
    // poll 读取持久化状态，使 WebUI 刷新后无需持有执行器 future 即可重新连接。
    const poll = async (): Promise<void> => {
      try {
        // job is the latest durable operation row for the exact selected library.
        // job 是精确选中知识库最近的持久化操作记录。
        const job = await getKnowledgeImportJob(selectedLibraryId);
        if (disposed) return;
        // active identifies both the persisted queue window and actual execution.
        // active 同时标识持久化排队窗口与实际执行阶段。
        const active = job?.state === "queued" || job?.state === "running";
        if (job && active) {
          trackedOperationJobId.current = job.id;
          handledTerminalJobId.current = null;
          setImportLibraryId(selectedLibraryId);
          setImportJob(job);
          setImportOutcome(null);
          setImportError("");
          setImportWarnings(job.warnings);
          if (dismissedOperationDialogJobId.current !== job.id) setImportProgressOpen(true);
          // ContentVisible identifies import phases reached only after authoritative rows and counts commit.
          // ContentVisible 标识仅在权威记录与计数提交后才会到达的导入阶段。
          const contentVisible = knowledgeImportHasCommittedContent(job);
          if (contentVisible && projectedAuthoritativeJobId.current !== job.id) {
            projectedAuthoritativeJobId.current = job.id;
            // ProjectionRefresh exposes document and chunk counts while derived indexing is still active.
            // ProjectionRefresh 在派生索引仍运行时公开文档与切片计数。
            void Promise.allSettled([
              refreshLibraries(),
              refreshSources(selectedLibraryId),
            ]).then((results) => {
              if (results.some((result) => result.status === "rejected")
                && projectedAuthoritativeJobId.current === job.id) {
                projectedAuthoritativeJobId.current = null;
              }
            });
          }
          timer = window.setTimeout(() => void poll(), 500);
          return;
        }
        if (job && trackedOperationJobId.current === job.id) {
          setImportJob(job);
          setImportWarnings(job.warnings);
          setImportCancelRequested(false);
          setImportError(job.state === "failed" ? (job.errorMessage ?? t("knowledge.importDialog.failed")) : "");
          setImportOutcome(job.state === "completed" ? "completed" : (job.state === "cancelled" ? "cancelled" : "failed"));
          if (handledTerminalJobId.current !== job.id) {
            handledTerminalJobId.current = job.id;
            // TerminalRefresh reconciles every projection exactly once after detached work finishes.
            // TerminalRefresh 在分离式工作结束后仅一次协调全部投影。
            void Promise.allSettled([
              refreshLibraries(),
              refreshSources(selectedLibraryId),
              refreshIndexIntegrity(selectedLibraryId),
            ]);
          }
        }
      } catch (error) {
        if (disposed) return;
        // ActiveError remains visible once a concrete operation has been observed; initial recovery still retries quietly.
        // ActiveError 在观察到具体操作后保持可见；初始恢复失败则继续静默重试。
        if (trackedOperationJobId.current !== null) {
          setImportError(error instanceof Error ? error.message : String(error));
        }
        timer = window.setTimeout(() => void poll(), 1000);
      }
    };
    void poll();
    return () => {
      disposed = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [operationPollRevision, refreshIndexIntegrity, refreshLibraries, refreshSources, selectedLibraryId, t]);

  // startDirectoryImport closes the picker and hands execution to the retained backend runtime.
  // startDirectoryImport 关闭目录选择器并将执行交给保留的后端运行时。
  const startDirectoryImport = React.useCallback(async (libraryId: string, path: string, sourceId: string | null = null) => {
    if (!acquireBusy("directory-import")) return;
    setSourcePickerTarget(null);
    setImportLibraryId(libraryId);
    // TrackedOperation is cleared before an ambiguous start response can expose an older terminal row.
    // TrackedOperation 在可能出现不确定启动响应前清空，避免暴露旧终态记录。
    trackedOperationJobId.current = null;
    dismissedOperationDialogJobId.current = null;
    handledTerminalJobId.current = null;
    projectedAuthoritativeJobId.current = null;
    setImportJob(null);
    setImportError("");
    setImportOutcome(null);
    setImportCancelRequested(false);
    setImportWarnings([]);
    setImportProgressOpen(true);
    try {
      // job is persisted before the service returns, so polling never races a missing operation row.
      // job 在服务返回前已经持久化，因此轮询不会与缺失的操作记录发生竞态。
      const job = await importKnowledgeDirectory(libraryId, path, sourceId);
      trackedOperationJobId.current = job.id;
      handledTerminalJobId.current = null;
      setImportJob(job);
      setImportWarnings(job.warnings);
      setOperationPollRevision((revision) => revision + 1);
    } catch (error) {
      setImportError(error instanceof Error ? error.message : String(error));
      setImportOutcome("failed");
      // RecoveryPoll resolves whether the service accepted the durable job before the response failed.
      // RecoveryPoll 用于确认服务是否在响应失败前已经接受持久化任务。
      setOperationPollRevision((revision) => revision + 1);
    } finally {
      releaseBusy("directory-import");
    }
  }, [acquireBusy, releaseBusy]);

  // requestImportCancellation acknowledges the click immediately and requests cooperative cancellation once.
  // requestImportCancellation 立即确认点击，并仅请求一次协作取消。
  const requestImportCancellation = React.useCallback(async () => {
    if (!importLibraryId || !importJob?.id || importCancelRequested) return;
    setImportCancelRequested(true);
    setImportError("");
    try {
      const accepted = await cancelKnowledgeImport(importLibraryId, importJob.id);
      if (!accepted) {
        setImportCancelRequested(false);
      }
    } catch (error) {
      setImportCancelRequested(false);
      setImportError(error instanceof Error ? error.message : String(error));
    }
  }, [importCancelRequested, importJob?.id, importLibraryId]);

  // startIndexOperation starts one detached repair or rebuild and reuses the durable progress dialog.
  // startIndexOperation 启动一个分离式补全或重建，并复用持久化进度对话框。
  // Parameters: operation selects repair or rebuild and libraryId identifies the exact target.
  // 参数：operation 选择补全或重建，libraryId 标识精确目标。返回值：操作启动完成后的 Promise。
  const startIndexOperation = React.useCallback(async (operation: "repair" | "rebuild", libraryId: string): Promise<void> => {
    if (!acquireBusy(operation)) return;
    setImportLibraryId(libraryId);
    // TrackedOperation is cleared before an ambiguous start response can expose an older terminal row.
    // TrackedOperation 在可能出现不确定启动响应前清空，避免暴露旧终态记录。
    trackedOperationJobId.current = null;
    dismissedOperationDialogJobId.current = null;
    handledTerminalJobId.current = null;
    projectedAuthoritativeJobId.current = null;
    setImportJob(null);
    setImportError("");
    setImportOutcome(null);
    setImportCancelRequested(false);
    setImportWarnings([]);
    setImportProgressOpen(true);
    try {
      // job is the durable queue row returned before detached indexing proceeds.
      // job 是分离式索引继续执行前返回的持久化排队记录。
      const job = operation === "repair"
        ? await repairKnowledgeIndex(libraryId)
        : await rebuildKnowledgeIndex(libraryId);
      trackedOperationJobId.current = job.id;
      handledTerminalJobId.current = null;
      setImportJob(job);
      setOperationPollRevision((revision) => revision + 1);
    } catch (error) {
      setImportError(error instanceof Error ? error.message : String(error));
      setImportOutcome("failed");
      // RecoveryPoll resolves whether the service accepted the durable job before the response failed.
      // RecoveryPoll 用于确认服务是否在响应失败前已经接受持久化任务。
      setOperationPollRevision((revision) => revision + 1);
    } finally {
      releaseBusy(operation);
    }
  }, [acquireBusy, releaseBusy]);

  // startDerivedOperation starts one detached display or vector build and reuses durable polling.
  // startDerivedOperation 启动一个分离式显示库或向量构建，并复用持久化轮询。
  // Parameters: operation selects the derived stage and libraryId identifies the exact target.
  // 参数：operation 选择派生阶段，libraryId 标识精确目标。
  const startDerivedOperation = React.useCallback(async (operation: "display-build" | "vector-build", libraryId: string): Promise<void> => {
    if (!acquireBusy(operation)) return;
    setImportLibraryId(libraryId);
    trackedOperationJobId.current = null;
    dismissedOperationDialogJobId.current = null;
    handledTerminalJobId.current = null;
    projectedAuthoritativeJobId.current = null;
    setImportJob(null);
    setImportError("");
    setImportOutcome(null);
    setImportCancelRequested(false);
    setImportWarnings([]);
    setImportProgressOpen(true);
    try {
      const job = operation === "display-build"
        ? await buildKnowledgeDisplayIndex(libraryId)
        : await buildKnowledgeVectorIndex(libraryId);
      trackedOperationJobId.current = job.id;
      setImportJob(job);
      setOperationPollRevision((revision) => revision + 1);
    } catch (error) {
      setImportError(error instanceof Error ? error.message : String(error));
      setImportOutcome("failed");
      setOperationPollRevision((revision) => revision + 1);
    } finally {
      releaseBusy(operation);
    }
  }, [acquireBusy, releaseBusy]);

  // reopenDerivedOperationProgress reconnects the dialog to a detached operation without starting another build.
  // reopenDerivedOperationProgress 将弹窗重新连接到分离式任务，且不会启动另一轮构建。
  // Parameters: libraryId identifies the exact library whose durable job should be polled. Returns: nothing.
  // 参数：libraryId 标识需要轮询持久化任务的精确资料库。返回值：无。
  const reopenDerivedOperationProgress = React.useCallback((libraryId: string): void => {
    dismissedOperationDialogJobId.current = null;
    setImportLibraryId(libraryId);
    setImportJob(null);
    setImportError("");
    setImportOutcome(null);
    setImportCancelRequested(false);
    setImportWarnings([]);
    setImportProgressOpen(true);
    setOperationPollRevision((revision) => revision + 1);
  }, []);

  // hideOperationProgress keeps detached execution running while hiding only the progress projection.
  // hideOperationProgress 保持分离式执行继续运行，同时仅隐藏进度投影。
  // Returns: nothing after the current durable job is marked as intentionally hidden.
  // 返回值：将当前持久化任务标记为主动隐藏后不返回内容。
  const hideOperationProgress = React.useCallback((): void => {
    if (operationDialogRunning && importJob?.id) {
      dismissedOperationDialogJobId.current = importJob.id;
    }
    setImportProgressOpen(false);
  }, [importJob?.id, operationDialogRunning]);

  // runMutation serializes page mutations and refreshes authoritative registry state.
  // runMutation 串行化页面变更并刷新权威注册状态。
  const runMutation = React.useCallback(async (operation: KnowledgeBusyOperation, label: string, action: () => Promise<string | void>) => {
    if (!acquireBusy(operation)) return;
    try {
      const successDetail = await action();
      await refreshLibraries();
      toast.success(successDetail ?? t("knowledge.operationCompleted").replace("{operation}", label));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      releaseBusy(operation);
    }
  }, [acquireBusy, refreshLibraries, releaseBusy, t]);

  // refreshLibraryList reloads registry state with local busy feedback and error toast ownership.
  // refreshLibraryList 使用本地忙碌反馈与错误 Toast 归属重新加载注册状态。
  const refreshLibraryList = React.useCallback(async () => {
    if (!acquireBusy("refresh")) return;
    try {
      await refreshLibraries();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      releaseBusy("refresh");
    }
  }, [acquireBusy, refreshLibraries, releaseBusy]);

  // openCreateDialog loads the current official model capability snapshot before showing choices.
  // openCreateDialog 在显示选项前加载当前官方模型能力快照。
  const openCreateDialog = React.useCallback(async () => {
    try {
      const snapshot = await loadPublicAiSettings();
      setCreateInferenceCandidates(snapshot.internalInferenceCandidates);
      setCreateRetrievalMode("raw_bm25");
      setCreateCurationCandidateKey("");
      setCreateCurationReasoningEffort("");
      setCreateEmbeddingCandidateKey("");
      setCreateConfiguredDimension("");
      setCreateRequestedBatchSize("");
      setCreateCompatibilityOverride("");
      setCreateDialogOpen(true);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, []);

  // openSettingsDialog loads fresh official candidates and mirrors the persisted library choices.
  // openSettingsDialog 加载最新官方候选并镜像持久化的资料库选择。
  const openSettingsDialog = React.useCallback(async () => {
    if (!selectedLibrary) return;
    try {
      const snapshot = await loadPublicAiSettings();
      const settings = selectedLibrary.settings;
      setCreateInferenceCandidates(snapshot.internalInferenceCandidates);
      setCreateName(selectedLibrary.name);
      setCreateRetrievalMode(settings.retrievalMode);
      const routeKeyForSettings = (route: { providerId: string; serviceEntryId: string; connectionId?: string | null; modelId: string; protocol: string; bindingKind?: "protocol" | "action" } | null | undefined, operation: string) => route ? snapshot.internalInferenceCandidates.find((candidate) => candidate.route.operation === operation && candidate.route.provider_id === route.providerId && candidate.route.service_entry_id === route.serviceEntryId && candidate.route.connection_id === route.connectionId && candidate.route.model_id === route.modelId && candidate.route.binding.kind === (route.bindingKind ?? "protocol") && (candidate.route.binding.kind === "protocol" ? candidate.route.binding.protocol_profile_id : candidate.route.binding.action_binding_id) === route.protocol) : null;
      const curationCandidate = routeKeyForSettings(settings.curation.route, "conversation_respond");
      const embeddingCandidate = routeKeyForSettings(settings.embedding.route, "embedding_create");
      setCreateCurationCandidateKey(curationCandidate ? knowledgeCandidateRouteKey(curationCandidate) : "");
      setCreateCurationReasoningEffort(settings.curation.reasoningEffort ?? "");
      setCreateEmbeddingCandidateKey(embeddingCandidate ? knowledgeCandidateRouteKey(embeddingCandidate) : "");
      setCreateConfiguredDimension(settings.embedding.configuredDimension ? String(settings.embedding.configuredDimension) : "");
      setCreateRequestedBatchSize(settings.embedding.embeddingBatchSize ? String(settings.embedding.embeddingBatchSize) : "");
      setCreateCompatibilityOverride(settings.vectorIndex.compatibilityOverride ?? "");
      setSettingsDialogOpen(true);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, [selectedLibrary]);

  // createLibrary validates and submits one named library with the selected model policy.
  // createLibrary 校验并提交一个带模型策略的命名知识库。
  const createLibrary = React.useCallback(async (model: KnowledgeLibraryCreateDialogModel) => {
    const name = createName.trim();
    if (!name) {
      toast.error(t("knowledge.nameRequired"));
      return;
    }
    const routeFromCandidate = (candidate: PublicAiInternalInferenceCandidate | null) => candidate ? ({
      consumerProfileId: "knowledge_base",
      providerId: candidate.route.provider_id,
      protocol: candidate.route.binding.kind === "protocol" ? candidate.route.binding.protocol_profile_id : candidate.route.binding.action_binding_id,
      bindingKind: candidate.route.binding.kind,
      serviceEntryId: candidate.route.service_entry_id,
      modelId: candidate.route.model_id,
      usageScope: "internal_background" as const,
      connectionId: candidate.route.connection_id,
    }) : null;
    const embeddingCapability = model.embeddingCandidate?.capabilities.embedding;
    await runMutation("create", t("knowledge.create"), async () => {
      const created = await createKnowledgeLibrary(name, "", [], {
        retrievalMode: model.retrievalMode,
        fallbackPolicy: "explicit_only",
        curation: {
          enabled: model.retrievalMode !== "raw_bm25",
          route: routeFromCandidate(model.curationCandidate),
          promptVersion: "knowledge-display-v1",
          reasoningEffort: model.reasoningEffort || null,
        },
        embedding: {
          enabled: model.retrievalMode === "display_hybrid",
          route: routeFromCandidate(model.embeddingCandidate),
          configuredDimension: model.configuredDimension,
          modelMaxBatchSize: embeddingCapability?.maximum_inputs ?? null,
          requestedBatchSize: model.requestedBatchSize,
          compatibilityOverride: model.compatibilityOverride.trim() || null,
        },
      });
      setCreateName("");
      setSelectedLibraryId(created.id);
      setCreateDialogOpen(false);
    });
  }, [createName, runMutation, t]);

  // updateLibrarySettings persists an edited policy without re-importing source files.
  // updateLibrarySettings 持久化编辑后的策略且不重新导入原始文件。
  const updateLibrarySettings = React.useCallback(async (model: KnowledgeLibraryCreateDialogModel) => {
    if (!selectedLibrary) return;
    const routeFromCandidate = (candidate: PublicAiInternalInferenceCandidate | null) => candidate ? ({
      consumerProfileId: "knowledge_base",
      providerId: candidate.route.provider_id,
      protocol: candidate.route.binding.kind === "protocol" ? candidate.route.binding.protocol_profile_id : candidate.route.binding.action_binding_id,
      bindingKind: candidate.route.binding.kind,
      serviceEntryId: candidate.route.service_entry_id,
      modelId: candidate.route.model_id,
      usageScope: "internal_background" as const,
      connectionId: candidate.route.connection_id,
    }) : null;
    const embeddingCapability = model.embeddingCandidate?.capabilities.embedding;
    await runMutation("settings", "保存资料库配置", async () => {
      await updateKnowledgeLibrarySettings(selectedLibrary.id, {
        retrievalMode: model.retrievalMode,
        fallbackPolicy: "explicit_only",
        curation: { enabled: model.retrievalMode !== "raw_bm25", route: routeFromCandidate(model.curationCandidate), promptVersion: "knowledge-display-v1", reasoningEffort: model.reasoningEffort || null },
        embedding: { enabled: model.retrievalMode === "display_hybrid", route: routeFromCandidate(model.embeddingCandidate), configuredDimension: model.configuredDimension, modelMaxBatchSize: embeddingCapability?.maximum_inputs ?? null, requestedBatchSize: model.requestedBatchSize, compatibilityOverride: model.compatibilityOverride.trim() || null },
      });
      setSettingsDialogOpen(false);
      await refreshLibraries();
    });
  }, [refreshLibraries, runMutation, selectedLibrary]);

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
      result.warnings.forEach((warning) => toast.warning(warning));
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
      ? { mode: "progress", label: "查看生成进度", disabled: false } satisfies KnowledgeDisplayBuildAction
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
          {filteredLibraries.map((library) => (
            <div className={library.id === selectedLibraryId ? "knowledge-library-row active" : "knowledge-library-row"} key={library.id}>
              <button className="knowledge-library-select" disabled={pageBusy} onClick={() => setSelectedLibraryId(library.id)} type="button">
                <Database size={18} />
                <span><strong>{library.name}</strong><small>{t("knowledge.libraryCounts").replace("{documents}", String(library.documentCount)).replace("{chunks}", String(library.chunkCount)).replace("{status}", library.indexStatus)}</small></span>
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
          {filteredLibraries.length === 0 ? <div className="knowledge-empty">{t("knowledge.emptyLibraries")}</div> : null}
        </aside>
        <main className="knowledge-detail">
          {selectedLibrary ? (
            <>
              <header className="knowledge-detail-header">
                <div className="knowledge-title-editor"><Input aria-label={t("knowledge.name")} disabled={pageBusy} onChange={(event) => setRenameDraft(event.target.value)} value={renameDraft} /><p>{selectedLibrary.description || t("knowledge.defaultDescription")}</p></div>
                <div className="knowledge-actions">
                  <Button disabled={pageBusy} onClick={() => void openSettingsDialog()} variant="outline"><Settings2 size={15} />配置</Button>
                  <Button disabled={pageBusy || !renameDraft.trim() || renameDraft.trim() === selectedLibrary.name} onClick={() => void runMutation("rename", t("knowledge.saveName"), async () => {
                    await renameKnowledgeLibrary(selectedLibrary.id, renameDraft.trim(), selectedLibrary.description);
                  })} variant="outline">{t("knowledge.saveName")}</Button>
                  <Button disabled={pageBusy} onClick={() => setSourcePickerTarget({ sourceId: null })}><FolderInput size={15} />{t("knowledge.sources.add")}</Button>
                  <Button disabled={pageBusy} onClick={() => {
                    if (window.confirm(t("knowledge.deleteConfirm").replace("{name}", selectedLibrary.name))) {
                      void runMutation("delete", t("knowledge.delete"), () => deleteKnowledgeLibrary(selectedLibrary.id));
                    }
                  }} variant="outline"><Trash2 size={15} />{t("knowledge.delete")}</Button>
                </div>
              </header>
              <section className="knowledge-index-health">
                <header className="knowledge-index-health-header">
                  <div>
                    <h3>{t("knowledge.indexHealth.title")}</h3>
                    <p>{t("knowledge.indexHealth.description")}</p>
                  </div>
                  {indexIntegrity ? (
                    <span className={`knowledge-index-health-state ${indexIntegrity.searchable ? "ready" : "attention"}`}>
                      {indexIntegrity.searchable ? t("knowledge.indexHealth.searchable") : t("knowledge.indexHealth.needsRepair")}
                    </span>
                  ) : null}
                </header>
                {indexIntegrityLoading ? <div className="knowledge-sources-state">{t("knowledge.indexHealth.loading")}</div> : null}
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
                    {selectedLibrary.settings.vectorIndex.compatibilityOverride ? <p className="text-sm text-amber-700">已人工确认同维度模型兼容，当前复用旧向量索引：{selectedLibrary.settings.vectorIndex.compatibilityOverride}</p> : null}
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
                        ><Wrench size={15} />{displayBuildAction?.label ?? "生成显示库"}</Button>
                      ) : null}
                      {selectedLibrary.settings.embedding.enabled ? (
                        <Button disabled={pageBusy || selectedLibrary.settings.displayIndex.status !== "ready" || selectedLibrary.settings.vectorIndex.status === "building"} onClick={() => void startDerivedOperation("vector-build", selectedLibrary.id)} variant="outline"><Database size={15} />生成向量</Button>
                      ) : null}
                    </div>
                  </>
                ) : null}
              </section>
              <section className="knowledge-sources">
                <header className="knowledge-sources-header">
                  <div>
                    <h3>{t("knowledge.sources.title")}</h3>
                    <p>{t("knowledge.sources.description")}</p>
                  </div>
                </header>
                {sourcesLoading ? <div className="knowledge-sources-state">{t("knowledge.sources.loading")}</div> : null}
                {!sourcesLoading && sources.length === 0 ? <div className="knowledge-sources-state">{t("knowledge.sources.empty")}</div> : null}
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
                              {t("knowledge.sources.lastScanned").replace("{time}", new Date(source.lastScannedAtMs).toLocaleString())}
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
          ) : <div className="knowledge-empty">{t("knowledge.emptySelection")}</div>}
        </main>
      </div>
      <KnowledgeLibraryCreateDialog
        candidates={createInferenceCandidates}
        configuredDimension={createConfiguredDimension}
        compatibilityOverride={createCompatibilityOverride}
        curationCandidateKey={createCurationCandidateKey}
        embeddingCandidateKey={createEmbeddingCandidateKey}
        reasoningEffort={createCurationReasoningEffort}
        name={createName}
        onConfiguredDimensionChange={setCreateConfiguredDimension}
        onCompatibilityOverrideChange={setCreateCompatibilityOverride}
        onCurationCandidateChange={setCreateCurationCandidateKey}
        onReasoningEffortChange={setCreateCurationReasoningEffort}
        onEmbeddingCandidateChange={setCreateEmbeddingCandidateKey}
        onNameChange={setCreateName}
        onOpenChange={setCreateDialogOpen}
        onRequestedBatchSizeChange={setCreateRequestedBatchSize}
        onRetrievalModeChange={setCreateRetrievalMode}
        onSubmit={(model) => void createLibrary(model)}
        open={createDialogOpen}
        requestedBatchSize={createRequestedBatchSize}
        retrievalMode={createRetrievalMode}
      />
      <KnowledgeLibraryCreateDialog
        candidates={createInferenceCandidates}
        configuredDimension={createConfiguredDimension}
        compatibilityOverride={createCompatibilityOverride}
        curationCandidateKey={createCurationCandidateKey}
        embeddingCandidateKey={createEmbeddingCandidateKey}
        reasoningEffort={createCurationReasoningEffort}
        name={createName}
        onConfiguredDimensionChange={setCreateConfiguredDimension}
        onCompatibilityOverrideChange={setCreateCompatibilityOverride}
        onCurationCandidateChange={setCreateCurationCandidateKey}
        onReasoningEffortChange={setCreateCurationReasoningEffort}
        onEmbeddingCandidateChange={setCreateEmbeddingCandidateKey}
        onNameChange={setCreateName}
        onOpenChange={setSettingsDialogOpen}
        onRequestedBatchSizeChange={setCreateRequestedBatchSize}
        onRetrievalModeChange={setCreateRetrievalMode}
        onSubmit={(model) => void updateLibrarySettings(model)}
        open={settingsDialogOpen}
        requestedBatchSize={createRequestedBatchSize}
        retrievalMode={createRetrievalMode}
        submitLabel="保存"
        title="资料库模型与检索配置"
      />
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
      <Dialog onOpenChange={(open) => {
        if (open) {
          setImportProgressOpen(true);
        } else if (!operationDialogRunning || importJob?.id) {
          hideOperationProgress();
        }
      }} open={importProgressOpen}>
        <DialogContent
          className="knowledge-import-dialog"
          onOpenAutoFocus={(event) => event.preventDefault()}
          showCloseButton={!operationDialogRunning || Boolean(importJob?.id)}
        >
          <DialogHeader>
            <DialogTitle>{displayedOperationKind === "repair"
              ? t("knowledge.operationDialog.title.repair")
              : (displayedOperationKind === "rebuild"
                ? t("knowledge.operationDialog.title.rebuild")
                : (displayedOperationKind === "display_build" ? "生成资料库显示层" : (displayedOperationKind === "vector_build" ? "生成资料库向量索引" : t("knowledge.operationDialog.title.import"))))}</DialogTitle>
          </DialogHeader>
          <KnowledgeOperationProgress cancelRequested={importCancelRequested} job={importJob} outcome={importOutcome} running={operationDialogRunning} />
          {importError ? <pre className="knowledge-import-dialog-error">{importError}</pre> : null}
          {importWarnings.length > 0 ? <pre className="knowledge-import-warnings">{importWarnings.join("\n")}</pre> : null}
          <DialogFooter>
            {operationDialogRunning && importLibraryId ? (
              <>
                <Button disabled={!importJob?.id} onClick={hideOperationProgress} type="button" variant="ghost">后台运行</Button>
                <Button disabled={!importJob?.id || importCancelRequested} onClick={() => void requestImportCancellation()} type="button" variant="outline">{importCancelRequested ? t("knowledge.importDialog.cancelling") : t(displayedOperationKind === "import" ? "knowledge.cancelImport" : "knowledge.cancelOperation")}</Button>
              </>
            ) : (
              <Button onClick={() => setImportProgressOpen(false)} type="button">{t("knowledge.importDialog.close")}</Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  );
}

// KnowledgeImportProgress renders durable import, repair, and rebuild progress states.
// KnowledgeImportProgress 渲染持久化导入、补全与重建进度状态。
function KnowledgeImportProgress(props: {
  // Whether the backend has accepted a cooperative cancellation request.
  // 后端是否已接受协作取消请求。
  cancelRequested: boolean;
  // Current durable import snapshot, absent before the backend creates the job row.
  // 当前持久化导入快照，后端创建任务行之前为空。
  job: KnowledgeImportJobSummary | null;
  // Terminal command result independent from optional progress refreshes.
  // 与可选进度刷新无关的终态命令结果。
  outcome: "completed" | "cancelled" | "failed" | null;
  // Whether the import command is still active.
  // 导入命令是否仍处于活动状态。
  running: boolean;
  // Localized text resolver owned by the page.
  // 页面持有的本地化文本解析器。
  t: ReturnType<typeof useI18n>["t"];
}) {
  // Percentage is intentionally indeterminate until the complete scan establishes a denominator.
  // 在完整扫描确定分母前，百分比刻意保持为不确定状态。
  const percentage = knowledgeImportProgressPercent(props.job);
  // IsImport distinguishes file discovery metrics from index-only maintenance metrics.
  // IsImport 区分文件发现指标与仅索引维护指标。
  const isImport = (props.job?.operationKind ?? "import") === "import";
  // IsPreparing distinguishes a durable queued row from actual filesystem or index work.
  // IsPreparing 区分持久化排队记录与实际文件系统或索引工作。
  const isPreparing = !props.job || props.job.phase === "queued";
  // IsScanning is derived only from the explicit scan phase so an empty completed scan is not mislabeled.
  // IsScanning 仅由显式扫描阶段派生，避免将已经完成的空目录扫描标错。
  const isScanning = isImport && props.job?.phase === "scanning";
  // IsIndexing identifies incremental or recovery FTS work with its own denominator.
  // IsIndexing 标识具有独立分母的增量或恢复 FTS 工作。
  const isIndexing = props.job?.phase === "indexing" || props.job?.phase === "repairing" || props.job?.phase === "rebuilding" || props.job?.phase === "curating" || props.job?.phase === "embedding";
  // Status describes the exact discovery, processing, or terminal state.
  // Status 描述准确的发现、处理或终态。
  const status = props.running
    ? (isPreparing
      ? props.t("knowledge.importDialog.preparing")
      : (props.cancelRequested
      ? props.t("knowledge.importDialog.cancellingDetail")
      : (isScanning
      ? props.t("knowledge.importDialog.scanning").replace("{discovered}", String(props.job?.scannedFiles ?? 0))
      : (isIndexing
        ? (props.job?.phase === "curating"
          ? `正在整理显示库 ${props.job?.processedIndexItems ?? 0} / ${props.job?.totalIndexItems ?? 0}（${percentage ?? 0}%）`
          : (props.job?.phase === "embedding"
            ? `正在生成向量 ${props.job?.processedIndexItems ?? 0} / ${props.job?.totalIndexItems ?? 0}（${percentage ?? 0}%）`
            : props.t(props.job?.phase === "repairing" ? "knowledge.importDialog.repairing" : (props.job?.phase === "rebuilding" ? "knowledge.importDialog.rebuilding" : "knowledge.importDialog.indexing")).replace("{processed}", String(props.job?.processedIndexItems ?? 0)).replace("{total}", String(props.job?.totalIndexItems ?? 0)).replace("{percent}", String(percentage ?? 0))))
        : importPhaseStatus(props.job, percentage ?? 0, props.t)))))
    : ((props.outcome ?? props.job?.state) === "completed"
      ? props.t("knowledge.importDialog.completed")
      : ((props.outcome ?? props.job?.state) === "cancelled" ? props.t("knowledge.importDialog.cancelled") : props.t("knowledge.importDialog.failed")));
  return (
    <div className="knowledge-import-dialog-body">
      <div className="knowledge-import-dialog-status"><strong>{status}</strong><span>{importPhaseLabel(props.job?.phase, props.t)}</span></div>
      <div aria-label={status} aria-valuemax={percentage === null ? undefined : 100} aria-valuemin={percentage === null ? undefined : 0} aria-valuenow={percentage ?? undefined} className={`knowledge-import-bar${percentage === null && props.running ? " indeterminate" : ""}`} role="progressbar">
        <span style={{ width: percentage === null ? undefined : `${percentage}%` }} />
      </div>
      <div className="knowledge-import-dialog-metrics">
        <span>{isIndexing || !isImport ? props.t("knowledge.importDialog.indexItems").replace("{processed}", String(props.job?.processedIndexItems ?? 0)).replace("{total}", String(props.job?.totalIndexItems ?? 0)) : (isScanning ? props.t("knowledge.importDialog.discovered").replace("{count}", String(props.job?.scannedFiles ?? 0)) : props.t("knowledge.importDialog.total").replace("{count}", String(props.job?.totalFiles ?? 0)))}</span>
        <span>{isImport ? props.t("knowledge.importDialog.bytes").replace("{count}", String(props.job?.processedBytes ?? 0)) : props.t("knowledge.indexHealth.tokenizer").replace("{tokenizer}", "jieba")}</span>
      </div>
      <div className="knowledge-import-current">
        <small>{props.t("knowledge.importDialog.current")}</small>
        <code>{props.job?.currentFile || importPhaseFallback(props.job?.phase, props.t)}</code>
      </div>
    </div>
  );
}

// KnowledgeTranslator is the exact localized text resolver used by knowledge progress helpers.
// KnowledgeTranslator 是知识库进度辅助函数使用的准确本地化文本解析器。
type KnowledgeTranslator = ReturnType<typeof useI18n>["t"];

// importPhaseStatus returns a truthful headline for non-scan, non-index phases.
// importPhaseStatus 为非扫描、非索引阶段返回真实标题。
function importPhaseStatus(job: KnowledgeImportJobSummary | null, percentage: number, t: KnowledgeTranslator): string {
  if (job?.phase === "committing") return t("knowledge.importDialog.committing");
  if (job?.phase === "verifying") return t("knowledge.importDialog.verifying");
  if (job?.phase === "finalizing") return t("knowledge.importDialog.finalizing");
  return t("knowledge.importDialog.processing")
    .replace("{processed}", String(job?.processedFiles ?? 0))
    .replace("{total}", String(job?.totalFiles ?? 0))
    .replace("{percent}", String(percentage));
}

// importPhaseLabel localizes the compact backend phase indicator.
// importPhaseLabel 本地化紧凑的后端阶段指示器。
function importPhaseLabel(phase: string | undefined, t: KnowledgeTranslator): string {
  switch (phase) {
    case "scanning": return t("knowledge.importPhase.scanning");
    case "parsing": return t("knowledge.importPhase.parsing");
    case "committing": return t("knowledge.importPhase.committing");
    case "indexing": return t("knowledge.importPhase.indexing");
    case "repairing": return t("knowledge.importPhase.repairing");
    case "rebuilding": return t("knowledge.importPhase.rebuilding");
    case "curating": return "整理显示库";
    case "embedding": return "生成向量";
    case "verifying": return t("knowledge.importPhase.verifying");
    case "finalizing": return t("knowledge.importPhase.finalizing");
    case "completed": return t("knowledge.importPhase.completed");
    case "cancelled": return t("knowledge.importPhase.cancelled");
    default: return t("knowledge.importDialog.preparing");
  }
}

// importPhaseFallback replaces the misleading scan wait message after file processing finishes.
// importPhaseFallback 在文件处理结束后替换具有误导性的扫描等待提示。
function importPhaseFallback(phase: string | undefined, t: KnowledgeTranslator): string {
  switch (phase) {
    case "committing": return t("knowledge.importDialog.committingFallback");
    case "indexing": return t("knowledge.importDialog.indexingFallback");
    case "repairing": return t("knowledge.importDialog.repairingFallback");
    case "rebuilding": return t("knowledge.importDialog.rebuildingFallback");
    case "curating": return "正在整理显示库";
    case "embedding": return "正在生成向量索引";
    case "verifying": return t("knowledge.importDialog.verifyingFallback");
    case "finalizing": return t("knowledge.importDialog.finalizingFallback");
    default: return t("knowledge.importDialog.waiting");
  }
}
