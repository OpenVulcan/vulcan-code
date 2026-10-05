import * as React from "react";

import type { useI18n } from "@/i18n";
import {
  buildKnowledgeDisplayIndex,
  buildKnowledgeVectorIndex,
  cancelKnowledgeImport,
  getKnowledgeImportJob,
  importKnowledgeDirectory,
  rebuildKnowledgeIndex,
  repairKnowledgeIndex,
} from "@/service/knowledge-base-service";
import type { KnowledgeFileConfirmation, KnowledgeImportJobSummary } from "@/types";
import type { KnowledgeBusyOperation, KnowledgeSourcePickerTarget } from "./knowledge-area-types";
import type { KnowledgeFileReviewTarget } from "./knowledge-file-confirmation-dialog";

// knowledgeImportHasCommittedContent identifies import phases reached after authoritative commit.
// knowledgeImportHasCommittedContent 标识在权威内容提交后才会到达的导入阶段。
// Parameters: job is one durable operation snapshot. Returns: whether document and chunk projections can refresh.
// 参数：job 是一份持久化操作快照。返回值：是否可以刷新文档与切片投影。
export function knowledgeImportHasCommittedContent(job: KnowledgeImportJobSummary): boolean {
  return job.operationKind === "import"
    && ["indexing", "rebuilding", "verifying", "finalizing"].includes(job.phase);
}

// KnowledgeOperationJobOptions supplies the page-owned mutation slot, projections and pickers the durable job drives.
// KnowledgeOperationJobOptions 提供持久化任务所驱动的页面变更槽位、投影与选择器。
export interface KnowledgeOperationJobOptions {
  // Exact library whose latest durable operation is polled.
  // 需要轮询最近持久化操作的精确知识库。
  selectedLibraryId: string | null;
  // Current page mutation owner, including the service-start window.
  // 当前页面变更所有者，包含服务启动窗口。
  busy: KnowledgeBusyOperation | null;
  // Claims the single page mutation slot; returns false when another operation owns it.
  // 占用唯一页面变更槽位；被其他操作占用时返回 false。
  acquireBusy: (operation: KnowledgeBusyOperation) => boolean;
  // Releases the mutation slot only for its exact owner.
  // 仅为精确所有者释放变更槽位。
  releaseBusy: (operation: KnowledgeBusyOperation) => void;
  // Reloads the authoritative library registry.
  // 重新加载权威知识库注册表。
  refreshLibraries: () => Promise<void>;
  // Reloads one library's source roots.
  // 重新加载一个知识库的来源根目录。
  refreshSources: (libraryId: string) => Promise<void>;
  // Reloads one library's index integrity snapshot.
  // 重新加载一个知识库的索引完整性快照。
  refreshIndexIntegrity: (libraryId: string) => Promise<void>;
  // Opens the file confirmation review before an import or display build starts.
  // 在导入或显示库构建开始前打开文件确认清单。
  setFileReview: React.Dispatch<React.SetStateAction<KnowledgeFileReviewTarget | null>>;
  // Closes the source directory picker once an import is handed off.
  // 导入交接后关闭来源目录选择器。
  setSourcePickerTarget: React.Dispatch<React.SetStateAction<KnowledgeSourcePickerTarget | null>>;
  // Localized text resolver owned by the page.
  // 页面持有的本地化文本解析器。
  t: ReturnType<typeof useI18n>["t"];
}

// KnowledgeOperationJob is the durable operation projection and its start, cancel and visibility actions.
// KnowledgeOperationJob 是持久化操作投影及其启动、取消与可见性动作。
export type KnowledgeOperationJob = ReturnType<typeof useKnowledgeOperationJob>;

// useKnowledgeOperationJob tracks one detached import, index or derived-build job through durable polling.
// useKnowledgeOperationJob 通过持久化轮询跟踪一个分离式导入、索引或派生构建任务。
// Parameters: options supplies page-owned state and refreshers. Returns: the job projection and its actions.
// 参数：options 提供页面持有的状态与刷新函数。返回值：任务投影及其动作。
export function useKnowledgeOperationJob({
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
}: KnowledgeOperationJobOptions) {
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
  // operationRunning is derived from durable backend state rather than the initiating HTTP request.
  // operationRunning 从持久化后端状态派生，而不是依赖发起操作的 HTTP 请求。
  const operationRunning = importJob?.state === "queued" || importJob?.state === "running";
  // operationDialogRunning includes the short service-start window before the durable job is returned.
  // operationDialogRunning 包含持久化任务返回前的短暂服务启动窗口。
  const operationDialogRunning = operationRunning || busy === "directory-import" || busy === "repair" || busy === "rebuild" || busy === "display-build" || busy === "vector-build";
  // displayedOperationKind preserves a truthful dialog title while a start request is in flight.
  // displayedOperationKind 在启动请求进行期间保持真实的对话框标题。
  const displayedOperationKind = importJob?.operationKind ?? (busy === "repair" ? "repair" : (busy === "rebuild" ? "rebuild" : (busy === "display-build" ? "display_build" : (busy === "vector-build" ? "vector_build" : "import"))));

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
  const startDirectoryImport = React.useCallback(async (libraryId: string, path: string, sourceId: string | null = null, confirmation?: KnowledgeFileConfirmation) => {
    if (!confirmation) {
      setSourcePickerTarget(null);
      setImportProgressOpen(false);
      setFileReview({ kind: "import", libraryId, sourceRoot: path, sourceId });
      return;
    }
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
      const job = await importKnowledgeDirectory(libraryId, path, sourceId, confirmation);
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
  const startDerivedOperation = React.useCallback(async (operation: "display-build" | "vector-build", libraryId: string, confirmation?: KnowledgeFileConfirmation): Promise<void> => {
    if (operation === "display-build" && !confirmation) {
      setImportProgressOpen(false);
      setFileReview({ kind: "display", libraryId });
      return;
    }
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
        ? await buildKnowledgeDisplayIndex(libraryId, confirmation!)
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

  return {
    importJob,
    importLibraryId,
    importProgressOpen,
    setImportProgressOpen,
    importError,
    importOutcome,
    importCancelRequested,
    importWarnings,
    operationRunning,
    operationDialogRunning,
    displayedOperationKind,
    startDirectoryImport,
    requestImportCancellation,
    startIndexOperation,
    startDerivedOperation,
    reopenDerivedOperationProgress,
    hideOperationProgress,
  };
}
