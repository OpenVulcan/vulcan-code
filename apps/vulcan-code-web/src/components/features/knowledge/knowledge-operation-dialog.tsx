import { RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useI18n } from "@/i18n";
import { KnowledgeOperationFeedback } from "./knowledge-feedback";
import { KnowledgeOperationProgress } from "./knowledge-operation-progress";
import type { KnowledgeOperationJob } from "./use-knowledge-operation-job";

// KnowledgeOperationDialog renders the non-blocking progress dialog of one durable knowledge operation.
// KnowledgeOperationDialog 渲染一个持久化知识库操作的非阻塞进度对话框。
// Parameters: operation is the page's durable job projection; pageBusy blocks a resume while the page is mutating.
// 参数：operation 是页面的持久化任务投影；pageBusy 在页面变更期间阻止继续生成。
export function KnowledgeOperationDialog({ operation, pageBusy }: {
  // Durable operation projection and actions owned by the knowledge page.
  // 知识库页面持有的持久化操作投影与动作。
  operation: KnowledgeOperationJob;
  // Whether a local request or durable operation currently owns the page.
  // 当前是否有本地请求或持久化操作占用页面。
  pageBusy: boolean;
}) {
  // t resolves dialog copy in the same locale as the owning knowledge page.
  // t 使用与所属知识库页面相同的语言解析弹窗文案。
  const { t } = useI18n();
  const {
    importJob,
    importLibraryId,
    importProgressOpen,
    setImportProgressOpen,
    importError,
    importOutcome,
    importCancelRequested,
    importWarnings,
    operationDialogRunning,
    displayedOperationKind,
    requestImportCancellation,
    startDerivedOperation,
    hideOperationProgress,
  } = operation;
  return (
    <Dialog onOpenChange={(open) => {
      if (open) {
        setImportProgressOpen(true);
      } else if (!operationDialogRunning || importJob?.id) {
        hideOperationProgress();
      }
    }} open={importProgressOpen}>
      <DialogContent
        className="knowledge-import-dialog"
        size="xl"
        layout="document"
        scrollMode="fixed-footer"
        showCloseButton={!operationDialogRunning || Boolean(importJob?.id)}
      >
        <DialogHeader className="shrink-0 border-b px-5 py-4 pr-12">
          <DialogTitle>{displayedOperationKind === "repair"
            ? t("knowledge.operationDialog.title.repair")
            : (displayedOperationKind === "rebuild"
              ? t("knowledge.operationDialog.title.rebuild")
              : (displayedOperationKind === "display_build" ? t("knowledge.experience.displayOperation") : (displayedOperationKind === "vector_build" ? t("knowledge.experience.vectorOperation") : t("knowledge.operationDialog.title.import"))))}</DialogTitle>
        </DialogHeader>
        <div className="knowledge-operation-dialog-body">
          {importError ? <KnowledgeOperationFeedback message={importError} /> : null}
          <KnowledgeOperationProgress cancelRequested={importCancelRequested} job={importJob} outcome={importOutcome} running={operationDialogRunning} />
          {importWarnings.length > 0 ? <details className="knowledge-operation-warnings"><summary>{t("knowledge.feedback.warnings").replace("{count}", String(importWarnings.length))}</summary><pre>{importWarnings.join("\n")}</pre></details> : null}
        </div>
        <DialogFooter className="m-0 shrink-0">
          {operationDialogRunning && importLibraryId ? (
            <>
              <Button disabled={!importJob?.id} onClick={hideOperationProgress} type="button" variant="ghost">{t("knowledge.experience.background")}</Button>
              <Button disabled={!importJob?.id || importCancelRequested} onClick={() => void requestImportCancellation()} type="button" variant="outline">{importCancelRequested ? t("knowledge.importDialog.cancelling") : t(displayedOperationKind === "import" ? "knowledge.cancelImport" : "knowledge.cancelOperation")}</Button>
            </>
          ) : (
            <>
              {importOutcome === "failed" && displayedOperationKind === "display_build" && importLibraryId ? <Button disabled={pageBusy} onClick={() => void startDerivedOperation("display-build", importLibraryId)} type="button" variant="outline"><RefreshCw size={14} />{t("knowledge.experience.resume")}</Button> : null}
              <Button onClick={() => setImportProgressOpen(false)} type="button" variant="outline">{t("knowledge.importDialog.close")}</Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
