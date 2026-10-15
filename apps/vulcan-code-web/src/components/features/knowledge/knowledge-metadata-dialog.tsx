import * as React from "react";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/i18n";
import type { KnowledgeLibraryRenameRequest, KnowledgeLibrarySummary } from "@/types/knowledge-base";

// Library identity and purpose are the only editable metadata on this human surface.
// 此人工界面仅可编辑知识库身份及用途元信息。
interface KnowledgeMetadataDialogProps {
  // Immutable owner chosen when opening this dialog.
  // 打开此弹窗时选定的不可变所有者。
  library: KnowledgeLibrarySummary;
  // Parent serialization blocks overlapping submissions.
  // 父级串行化阻止重叠提交。
  busy: boolean;
  // Dismisses this mounted draft.
  // 关闭此挂载草稿。
  onClose: () => void;
  // Saves name/purpose only and resolves after the registry refresh attempt.
  // 仅保存名称及用途，在注册表刷新尝试后完成。
  onSave: (request: KnowledgeLibraryRenameRequest) => Promise<void>;
}

// Edits human library metadata without any content-editing endpoint.
// 编辑人工知识库元信息，不使用任何内容编辑端点。
// Parameters: props freezes the library and save callback. Returns the controlled metadata form.
// 参数：props 冻结知识库及保存回调。返回受控元信息表单。
export function KnowledgeMetadataDialog({ library, busy, onClose, onSave }: KnowledgeMetadataDialogProps) {
  // Labels come from the current shared locale.
  // 标签来自当前共享语言。
  const { t } = useI18n();
  // Name belongs to this exact mounted library draft.
  // 名称属于此精确挂载知识库草稿。
  const [name, setName] = React.useState(library.name);
  // Purpose informs AI content maintenance.
  // 用途指导 AI 内容维护。
  const [description, setDescription] = React.useState(library.description);
  // A failed save preserves both draft and diagnostic.
  // 保存失败同时保留草稿及诊断。
  const [error, setError] = React.useState<string | null>(null);
  // Synchronous ownership prevents double submission before rendering busy.
  // 同步归属在渲染忙碌前防止重复提交。
  const saving = React.useRef(false);
  // Persists one exact metadata draft; returns after the parent-owned save attempt.
  // 持久化一个精确元信息草稿；父级拥有的保存尝试后返回。
  // Parameters: none. Returns after a successful save or a retained error.
  // 参数：无。成功保存或保留错误后返回。
  async function submit(): Promise<void> {
    if (busy || saving.current || !name.trim()) return;
    saving.current = true;
    setError(null);
    try { await onSave({ libraryId: library.id, name: name.trim(), description }); onClose(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { saving.current = false; }
  }
  return <Dialog open onOpenChange={(open) => { if (!open && !busy && !saving.current) onClose(); }}><DialogContent>
    <DialogHeader><DialogTitle>{t("knowledge.metadata")}</DialogTitle><DialogDescription>{t("knowledge.metadataDescription")}</DialogDescription></DialogHeader>
    <label className="grid gap-1">{t("knowledge.name")}<Input aria-label={t("knowledge.name")} value={name} disabled={busy} onChange={(event) => setName(event.target.value)} /></label>
    <label className="grid gap-1">{t("knowledge.purpose")}<Input aria-label={t("knowledge.purpose")} value={description} disabled={busy} onChange={(event) => setDescription(event.target.value)} /></label>
    {error !== null ? <pre role="alert" className="select-text whitespace-pre-wrap break-words text-sm text-destructive">{error}</pre> : null}
    <DialogFooter><Button variant="outline" disabled={busy} onClick={onClose}>{t("conversation.action.cancel")}</Button><Button disabled={busy || !name.trim()} onClick={() => void submit()}>{t("memory.common.save")}</Button></DialogFooter>
  </DialogContent></Dialog>;
}
