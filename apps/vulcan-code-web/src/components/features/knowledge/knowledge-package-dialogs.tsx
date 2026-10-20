import * as React from "react";
import { PackageFilePickerDialog } from "@/components/project/package-file-picker-dialog";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Input } from "@/ui-kit/primitives/input";
import { useI18n } from "@/i18n";
import { browseKnowledgePackages } from "@/service/knowledge-base-service";
import type { KnowledgePackageInspection } from "@/types/knowledge-base";
import { KNOWLEDGE_MODE_COPY } from "./knowledge-mode";

// Parameters: props owns picker visibility and exact-file inspection. Returns the .vkb package picker.
// 参数：props 拥有选择器可见性及精确文件检查。返回 .vkb 知识库包选择器。
export function KnowledgePackagePickerDialog(props: {
  // Whether the package picker is visible.
  // 知识库包选择器是否可见。
  open: boolean;
  // Handles picker visibility changes.
  // 处理选择器可见性变化。
  onOpenChange: (open: boolean) => void;
  // Inspects the exact package selected by the user.
  // 检查用户选择的精确知识库包。
  onSelect: (path: string) => Promise<void>;
}) {
  const { t } = useI18n();
  return <PackageFilePickerDialog {...props} onBrowse={browseKnowledgePackages} title={t("knowledge.package.importPickerTitle")}
    emptyLabel={t("knowledge.package.noPackages")} confirmLabel={t("knowledge.package.inspect")} busyLabel={t("knowledge.package.inspecting")} />;
}

// A reviewed archive may only create a new independent library.
// 已审阅归档仅可创建新的独立知识库。
interface KnowledgePackageImportDialogProps {
  // Frozen exact path, complete byte fingerprint and validated manifest.
  // 冻结精确路径、完整字节指纹及已验证清单。
  inspection: KnowledgePackageInspection;
  // Parent mutation state blocks overlapping imports.
  // 父级变更状态阻止重叠导入。
  busy: boolean;
  // Closes the reviewed archive without executing it.
  // 关闭已审阅归档，不执行导入。
  onClose: () => void;
  // Imports exact reviewed bytes under name; returns after the committed creation and refresh attempt.
  // 以 name 导入精确已审阅字节；提交创建及刷新尝试后返回。
  onImport: (name: string) => Promise<void>;
}

// Confirms a current-format content package, with no replace-existing or legacy compatibility path.
// 确认当前格式内容套件，不提供替换既有库或旧版兼容入口。
// Parameters: props supplies the immutable inspection and explicit create callback. Returns name, cost disclosure and confirmation.
// 参数：props 提供不可变检查及明确创建回调。返回名称、成本说明及确认。
export function KnowledgePackageImportDialog({ inspection, busy, onClose, onImport }: KnowledgePackageImportDialogProps) {
  // Visible package labels use the application locale.
  // 可见套件标签使用应用语言。
  const { t } = useI18n();
  // The manifest is the sole source of preview metadata.
  // 清单为预览元数据唯一来源。
  const manifest = inspection.manifest;
  // Names are human-editable while every imported byte remains fingerprint-bound.
  // 名称可由人工编辑，而全部导入字节保持指纹绑定。
  const [name, setName] = React.useState(manifest.name);
  // Failed imports retain their exact reviewed identity and diagnosis.
  // 导入失败保留精确已审阅身份及诊断。
  const [error, setError] = React.useState<string | null>(null);
  // Prevent duplicate confirmation before parent busy publication.
  // 在父级忙碌发布前防止重复确认。
  const submitting = React.useRef(false);
  // Executes one exact reviewed import and preserves errors; returns after the parent-owned transaction.
  // 执行一次精确已审阅导入并保留错误；父级拥有事务后返回。
  async function submit(): Promise<void> {
    if (busy || submitting.current || !name.trim()) return;
    submitting.current = true;
    setError(null);
    try { await onImport(name.trim()); onClose(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { submitting.current = false; }
  }
  return <Dialog open onOpenChange={(open) => { if (!open && !busy && !submitting.current) onClose(); }}>
    <DialogContent>
      <DialogHeader><DialogTitle>{t("knowledge.package.import")}</DialogTitle><DialogDescription>{t("knowledge.package.independent")}</DialogDescription></DialogHeader>
      <div className="vk-field-group">
        <p className="vk-mono vk-wrap-pre">{inspection.archivePath}</p>
        <p>{manifest.description}</p>
        <p>{t("knowledge.package.contentCounts").replace("{documents}", String(manifest.documentCount)).replace("{folders}", String(manifest.folderCount))}</p>
        <p>{t(KNOWLEDGE_MODE_COPY[manifest.settings.indexMode].label)}</p>
        <p className="vk-field-hint">{t(KNOWLEDGE_MODE_COPY[manifest.settings.indexMode].description)}</p>
        <label className="vk-field">{t("knowledge.name")}<Input aria-label={t("knowledge.name")} disabled={busy} value={name} onChange={(event) => setName(event.target.value)} /></label>
        {error !== null ? <pre role="alert" className="vk-diag">{error}</pre> : null}
      </div>
      <DialogFooter><Button variant="outline" disabled={busy} onClick={onClose}>{t("conversation.action.cancel")}</Button><Button disabled={busy || !name.trim()} onClick={() => void submit()}>{t("knowledge.package.import")}</Button></DialogFooter>
    </DialogContent>
  </Dialog>;
}
