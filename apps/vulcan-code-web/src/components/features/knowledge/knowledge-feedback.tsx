import * as React from "react";
import { AlertCircle, Copy } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { PageNotice } from "@/components/page";
import { useI18n } from "@/i18n";

// Keeps exact read errors separate from empty results and exposes only an explicit read retry.
// 将精确读取错误与空结果区分，仅提供显式读取重试。
// Parameters: error is the diagnostic; onRetry repeats the owning read; busy blocks overlap. Returns a selectable error banner.
// 参数：error 为诊断；onRetry 重复所属读取；busy 阻止重叠。返回可选择错误提示条。
export function KnowledgeReadFailure({ error, onRetry, busy = false }: { error: string; onRetry: () => void; busy?: boolean }) {
  // Labels share the page locale; diagnostic content remains unchanged.
  // 标签共享页面语言；诊断内容保持原样。
  const { t } = useI18n();
  return <PageNotice
    tone="error"
    title={t("knowledge.readFailed")}
    details={error}
    detailsLabel={t("toolPresentation.output.errorDetails")}
    actions={<Button type="button" variant="outline" size="sm" disabled={busy} onClick={() => onRetry()}>{t("knowledge.refresh")}</Button>}
  />;
}

// KnowledgeMessageDialog shows a complete, selectable diagnostic within the current application window.
// KnowledgeMessageDialog 在当前应用窗口内显示完整、可选择的诊断内容。
// Parameters: message is the exact diagnostic or null; onClose dismisses it; restoreFocus optionally restores the owning control. Returns the shared dialog.
// 参数：message 是精确诊断或 null；onClose 关闭提示；restoreFocus 可选地恢复所属控件焦点。返回共享弹窗。
export function KnowledgeMessageDialog({ message, onClose, restoreFocus }: { message: string | null; onClose: () => void; restoreFocus?: () => void }) {
  // t resolves feedback labels in the active locale.
  // t 使用当前语言解析反馈文案。
  const { t } = useI18n();
  // copyState reports clipboard success or failure without losing the original diagnostic.
  // copyState 报告剪贴板成功或失败，同时保留原始诊断。
  const [copyState, setCopyState] = React.useState<"idle" | "copied" | "failed">("idle");
  React.useEffect(() => setCopyState("idle"), [message]);
  // copyDetails copies the unmodified message; returns after the clipboard operation finishes.
  // copyDetails 复制未经改写的提示；剪贴板操作结束后返回。
  const copyDetails = async (): Promise<void> => {
    if (message === null) return;
    try { await navigator.clipboard.writeText(message); setCopyState("copied"); }
    catch { setCopyState("failed"); }
  };
  return (
    <Dialog open={message !== null} onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent size="lg" layout="document" scrollMode="fixed-footer" className="knowledge-message-dialog" onCloseAutoFocus={restoreFocus ? (event) => { event.preventDefault(); restoreFocus(); } : undefined}>
        <DialogHeader>
          <DialogTitle>{t("knowledge.feedback.title")}</DialogTitle>
          <DialogDescription>{t("knowledge.feedback.description")}</DialogDescription>
        </DialogHeader>
        <pre className="knowledge-message-details">{message}</pre>
        <DialogFooter>
          <span className="vk-field-hint" role="status">{copyState === "failed" ? t("knowledge.feedback.copyFailed") : ""}</span>
          <Button variant="outline" onClick={() => void copyDetails()}><Copy size={14} />{t(copyState === "copied" ? "knowledge.feedback.copied" : "knowledge.feedback.copy")}</Button>
          <Button variant="outline" onClick={onClose}>{t("knowledge.importDialog.close")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// KnowledgeOperationFeedback keeps a compact error in progress and opens new failures in a shared dialog.
// KnowledgeOperationFeedback 在进度中保留紧凑错误摘要，并使用共享弹窗显示新失败。
// Parameters: message is the exact operation error. Returns a persistent banner and dismissible details.
// 参数：message 是操作的精确错误。返回持久提示条及可关闭的详情。
export function KnowledgeOperationFeedback({ message }: { message: string }) {
  // t supplies localized summary and details labels.
  // t 提供本地化摘要及详情标签。
  const { t } = useI18n();
  // open announces each new error once; closing details leaves the persistent banner available.
  // open 对每条新错误提示一次；关闭详情后仍保留持久提示条。
  const [open, setOpen] = React.useState(true);
  // detailsButton restores focus to the progress dialog after automatic error presentation.
  // detailsButton 在自动错误提示关闭后将焦点恢复至进度弹窗。
  const detailsButton = React.useRef<HTMLButtonElement>(null);
  React.useEffect(() => setOpen(true), [message]);
  return (
    <>
      <div className="knowledge-operation-feedback" role="alert">
        <AlertCircle size={18} />
        <span>{t("knowledge.feedback.operationError")}</span>
        <Button ref={detailsButton} variant="outline" size="sm" onClick={() => setOpen(true)}>{t("knowledge.feedback.details")}</Button>
      </div>
      <KnowledgeMessageDialog message={open ? message : null} onClose={() => setOpen(false)} restoreFocus={() => detailsButton.current?.focus()} />
    </>
  );
}
