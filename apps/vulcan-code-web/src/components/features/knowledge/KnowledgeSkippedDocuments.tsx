import * as React from "react";
import { FileWarning } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import type { KnowledgeDisplaySkippedDocument } from "@/types";
import { KnowledgeMessageDialog } from "./KnowledgeFeedback";

// KnowledgeSkippedDocuments renders the full exclusion list without treating it as a build error.
// KnowledgeSkippedDocuments 展示完整忽略列表，不将其当作建库错误。
// Parameters: documents is the durable build/publication snapshot, absent on legacy rows; returns a notice and an in-window copyable list.
// 参数：documents 是持久化构建或发布快照，旧记录可缺失；返回提醒及窗口内可复制清单。
export function KnowledgeSkippedDocuments({ documents }: { documents?: KnowledgeDisplaySkippedDocument[] }) {
  // t supplies localized notices and size labels.
  // t 提供本地化提醒及大小标签。
  const { t } = useI18n();
  // open controls the existing in-window diagnostic component.
  // open 控制现有窗口内详情组件。
  const [open, setOpen] = React.useState(false);
  // trigger restores focus to the button that opened the list.
  // trigger 将焦点恢复到打开列表的按钮。
  const trigger = React.useRef<HTMLButtonElement>(null);
  if (!documents?.length) return null;
  // summary states the omission count explicitly, including after a completed build.
  // summary 明确说明忽略数量，建库完成后仍然显示。
  const summary = t("knowledge.skipped.summary").replace("{count}", String(documents.length));
  // rows preserve every filename and exact byte size, without preview truncation.
  // rows 保留全部文件名和准确字节大小，不按预览条数截断。
  const rows = documents.map((document) => `${document.relativePath}\n${document.reason === "ignored" ? t("knowledge.files.ignored") : document.reason === "not_selected" ? t("knowledge.files.notSelected") : t("knowledge.skipped.size").replace("{size}", String(document.byteSize)).replace("{limit}", String(document.byteLimit))}`);
  return (
    <section className="min-w-0 rounded-lg border border-amber-500/40 bg-amber-500/5 p-3 text-sm" aria-label={summary}>
      <div className="flex flex-wrap items-center gap-2" role="status">
        <FileWarning size={16} className="shrink-0 text-amber-600" />
        <strong className="min-w-0 flex-1">{summary}</strong>
        <Button ref={trigger} size="sm" variant="outline" onClick={() => setOpen(true)}>{t("knowledge.skipped.details")}</Button>
      </div>
      <p className="my-2 text-muted-foreground">{t("knowledge.skipped.hint")}</p>
      <ul className="max-h-40 space-y-2 overflow-y-auto" aria-label={t("knowledge.skipped.details")}>
        {documents.map((document, index) => <li className="whitespace-pre-wrap break-all font-mono text-xs" key={document.sourceDocumentId}>{rows[index]}</li>)}
      </ul>
      <KnowledgeMessageDialog message={open ? `${summary}\n\n${t("knowledge.skipped.hint")}\n\n${rows.join("\n\n")}` : null} onClose={() => setOpen(false)} restoreFocus={() => trigger.current?.focus()} />
    </section>
  );
}
