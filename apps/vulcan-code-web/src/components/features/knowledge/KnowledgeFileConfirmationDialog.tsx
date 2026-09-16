import * as React from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useI18n, type TranslationKey } from "@/i18n";
import { previewKnowledgeImport, previewKnowledgeDisplay, updateKnowledgeIgnoredFiles } from "@/service/knowledge-base-service";
import type { KnowledgeFileConfirmation, KnowledgeFileDisposition, KnowledgeFilePreview, KnowledgeIgnoredFile } from "@/types";
import { KnowledgeOperationFeedback } from "./KnowledgeFeedback";

// KnowledgeFileReviewTarget freezes the exact library and operation being confirmed.
// KnowledgeFileReviewTarget 冻结正在确认的精确知识库和操作。
export type KnowledgeFileReviewTarget = {
  // Exact library owner.
  // 精确知识库归属。
  libraryId: string;
} & ({ kind: "import"; sourceRoot: string; sourceId: string | null } | { kind: "display" | "ignored" });

// DISPOSITION_LABELS projects protocol reasons without inferring eligibility from display text.
// DISPOSITION_LABELS 映射协议原因，不从展示文案推测可用性。
const DISPOSITION_LABELS: Record<KnowledgeFileDisposition, TranslationKey> = {
  ready: "knowledge.files.ready", completed: "knowledge.files.completed", oversized: "knowledge.files.oversized",
  ignored: "knowledge.files.ignored", unsupported: "knowledge.files.unsupported", unreadable: "knowledge.files.unreadable", not_selected: "knowledge.files.notSelected",
};

// ignoreIdentity identifies a UI row by exact stored root and path, not just the basename.
// ignoreIdentity 根据精确持久根目录和路径标识界面记录，不只使用文件名。
function ignoreIdentity(file: KnowledgeIgnoredFile): string { return JSON.stringify([file.sourceRoot, file.relativePath]); }

// KnowledgeFileConfirmationDialog previews before execution and manages reversible ignores.
// KnowledgeFileConfirmationDialog 在执行前预览，并管理可撤销忽略项。
// Parameters: target fixes the operation, ignoredFiles supplies current rules, onConfirm starts confirmed work, onRulesChanged refreshes registry state, onClose dismisses without executing.
// 参数：target 固定操作，ignoredFiles 提供当前规则，onConfirm 启动已确认工作，onRulesChanged 刷新目录状态，onClose 关闭且不执行。
export function KnowledgeFileConfirmationDialog({ target, ignoredFiles, onConfirm, onRulesChanged, onClose }: {
  // Frozen operation scope.
  // 冻结的操作范围。
  target: KnowledgeFileReviewTarget;
  // Current library-owned persistent ignores.
  // 当前知识库持有的持久忽略项。
  ignoredFiles: KnowledgeIgnoredFile[];
  // Explicit confirmation callback, never called automatically after scanning.
  // 明确确认回调，扫描完成后绝不自动调用。
  onConfirm: (confirmation: KnowledgeFileConfirmation) => void;
  // Refreshes persisted library metadata after a rule change.
  // 规则变更后刷新持久化知识库元数据。
  onRulesChanged: () => Promise<unknown>;
  // Closes the review without starting any work.
  // 关闭检查，不启动任何工作。
  onClose: () => void;
}) {
  // t resolves all operation and policy labels.
  // t 解析所有操作与策略标签。
  const { t } = useI18n();
  // preview is authoritative; a failed refresh discards the previous confirmation fingerprint.
  // preview 为权威快照，刷新失败时丢弃旧确认指纹。
  const [preview, setPreview] = React.useState<KnowledgeFilePreview | null>(null);
  // selected contains only ready keys from the current snapshot.
  // selected 仅包含当前快照中的可用键。
  const [selected, setSelected] = React.useState<Set<string>>(new Set());
  // choices retains deliberate unchecks across rule-triggered rescans without reselecting excluded work.
  // choices 在规则触发重新扫描时保留明确取消勾选，避免重新选中排除的工作。
  const choices = React.useRef(new Map<string, boolean>());
  // rules remains visible even when ignored files no longer exist on disk.
  // rules 在已忽略文件不再存在于磁盘时仍然可见。
  const [rules, setRules] = React.useState(ignoredFiles);
  // removal is separate from execution selection because ignored rows are never executable.
  // removal 与执行选择分离，因为已忽略记录绝不可执行。
  const [removal, setRemoval] = React.useState<Set<string>>(new Set());
  // tab selects the preview or the durable ignore registry.
  // tab 选择预览或持久忽略目录。
  const [tab, setTab] = React.useState<"files" | "ignored">(target.kind === "ignored" ? "ignored" : "files");
  // query and page limit rendered rows without truncating the confirmed scope.
  // query 和 page 限制渲染记录，不截断确认范围。
  const [query, setQuery] = React.useState("");
  // page is one-based within the current filtered list.
  // page 是当前筛选清单内从一开始的页码。
  const [page, setPage] = React.useState(1);
  // scanning blocks confirmation but never prevents dismissing a read-only request.
  // scanning 阻止确认，但绝不阻止关闭只读请求。
  const [scanning, setScanning] = React.useState(false);
  // savingRules locks dismissal only while an explicit persistent mutation is pending.
  // savingRules 仅在明确持久化变更尚未完成时禁止关闭。
  const [savingRules, setSavingRules] = React.useState(false);
  // busy serializes actions without conflating read-only waiting with persistent writes.
  // busy 串行化操作，不混淆只读等待与持久化写入。
  const busy = scanning || savingRules;
  // error preserves the exact failure in the shared in-window feedback component.
  // error 在共享窗口内反馈组件中保留精确失败。
  const [error, setError] = React.useState("");
  // sequence suppresses stale responses after dismissal or a newer scan.
  // sequence 在关闭或更新扫描后抑制过期响应。
  const sequence = React.useRef(0);
  // activeScan owns the abortable transport request for this exact dialog.
  // activeScan 持有当前精确弹窗的可中止传输请求。
  const activeScan = React.useRef<AbortController | null>(null);

  // cancelScan invalidates late results and aborts read-only I/O; returns without starting any work.
  // cancelScan 使延迟结果失效并中止只读 I/O，返回时不启动任何工作。
  const cancelScan = React.useCallback((): void => {
    sequence.current += 1;
    activeScan.current?.abort();
    activeScan.current = null;
  }, []);

  // dismiss permits leaving an unfinished scan, but not abandoning a pending ignore-rule write.
  // dismiss 允许退出未完成扫描，但不允许放弃尚未完成的忽略规则写入。
  const dismiss = (): void => {
    if (savingRules) return;
    cancelScan();
    onClose();
  };

  // scan returns a fresh fingerprint and defaults every eligible row to selected; it performs no import.
  // scan 返回新指纹并默认选中全部可用记录，不执行任何导入。
  const scan = React.useCallback(async (): Promise<void> => {
    if (target.kind === "ignored") return;
    cancelScan();
    // ticket binds state updates to this exact request.
    // ticket 将状态更新绑定到当前精确请求。
    const ticket = ++sequence.current;
    // controller propagates cancellation to the existing fetch-based service transport.
    // controller 将取消传递到现有基于 fetch 的服务传输。
    const controller = new AbortController();
    activeScan.current = controller;
    // deadline releases the UI even if a service request never returns.
    // deadline 即使服务请求一直不返回也会释放界面。
    let deadline: ReturnType<typeof setTimeout>;
    // stopped lets unmount/close settle the wait even when a transport result arrives late.
    // stopped 使卸载或关闭可以结束等待，即使传输结果延迟到达。
    const stopped = new Promise<never>((_, reject) => {
      controller.signal.addEventListener("abort", () => reject(controller.signal.reason), { once: true });
      deadline = setTimeout(() => controller.abort(new Error(t("knowledge.files.scanTimeout"))), 30_000);
    });
    setScanning(true); setError(""); setPreview(null); setSelected(new Set());
    try {
      // result is from the appropriate read-only operation endpoint.
      // result 来自对应的只读操作入口。
      const result = await Promise.race([
        target.kind === "import"
          ? previewKnowledgeImport(target.libraryId, target.sourceRoot, target.sourceId, controller.signal)
          : previewKnowledgeDisplay(target.libraryId, controller.signal),
        stopped,
      ]);
      if (ticket !== sequence.current) return;
      setPreview(result);
      setSelected(new Set(result.entries.filter((row) => row.disposition === "ready" && choices.current.get(row.key) !== false).map((row) => row.key)));
      setPage(1);
    } catch (reason) {
      if (ticket === sequence.current) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      clearTimeout(deadline!);
      if (ticket === sequence.current) { activeScan.current = null; setScanning(false); }
    }
  }, [target, t, cancelScan]);
  React.useEffect(() => { void scan(); return cancelScan; }, [scan, cancelScan]);

  // changeRules persists only an explicit user action; rescanning is required before further execution.
  // changeRules 仅持久化明确用户操作，继续执行前必须重新扫描。
  const changeRules = async (files: KnowledgeIgnoredFile[], ignored: boolean): Promise<void> => {
    if (busy || files.length === 0) return;
    setSavingRules(true); setError(""); setPreview(null);
    try {
      // next contains the complete updated library-scoped registry.
      // next 包含完整更新后的知识库范围规则目录。
      const next = await updateKnowledgeIgnoredFiles(target.libraryId, files, ignored);
      setRules(next); setRemoval(new Set());
      await onRulesChanged();
      setSavingRules(false);
      await scan();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setSavingRules(false); }
  };
  // eligible counts actionable rows independently of search or pagination.
  // eligible 独立于搜索和分页统计可执行记录。
  const eligible = preview?.entries.filter((row) => row.disposition === "ready") ?? [];
  // filtered lists use only user-visible paths and root identities.
  // 筛选列表仅使用用户可见路径和根目录身份。
  const filtered = (preview?.entries ?? []).filter((row) => `${row.sourceRoot}/${row.relativePath}`.toLowerCase().includes(query.toLowerCase()));
  // ignoredRows remains independent of the currently selected source directory.
  // ignoredRows 独立于当前选择的来源目录。
  const ignoredRows = rules.filter((row) => `${row.sourceRoot}/${row.relativePath}`.toLowerCase().includes(query.toLowerCase()));
  // count/pageCount reflect the current tab without hiding the total operation count.
  // count/pageCount 反映当前标签页，不隐藏操作总数。
  const count = tab === "files" ? filtered.length : ignoredRows.length;
  // At most one hundred rows are mounted even for a multi-thousand-file library.
  // 即使知识库包含数千文件，也最多挂载一百条记录。
  const pageCount = Math.max(1, Math.ceil(count / 100));
  // currentPage clamps only display pagination after a rule removal.
  // currentPage 仅在移除规则后约束展示页码。
  const currentPage = Math.min(page, pageCount);
  return <Dialog open onOpenChange={(open) => { if (!open) dismiss(); }}>
    <DialogContent size="xl" layout="document" scrollMode="fixed-footer" className="flex max-h-[calc(100dvh-2rem)] flex-col overflow-hidden p-0" showCloseButton={!savingRules}>
      <DialogHeader className="shrink-0 border-b px-5 py-4 pr-12">
        <DialogTitle>{t(target.kind === "ignored" ? "knowledge.files.manage" : "knowledge.files.title")}</DialogTitle>
        <DialogDescription>{t("knowledge.files.description")}</DialogDescription>
      </DialogHeader>
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-5 py-4">
        {error ? <KnowledgeOperationFeedback message={error} /> : null}
        <p className="text-sm text-muted-foreground">{t("knowledge.files.policy")}</p>
        <div className="flex flex-wrap gap-2">
          {target.kind !== "ignored" ? <Button disabled={busy} variant={tab === "files" ? "default" : "outline"} onClick={() => { setTab("files"); setPage(1); }}>{t("knowledge.files.all")} ({preview?.entries.length ?? 0})</Button> : null}
          <Button disabled={busy} variant={tab === "ignored" ? "default" : "outline"} onClick={() => { setTab("ignored"); setPage(1); }}>{t("knowledge.files.ignored")} ({rules.length})</Button>
          {target.kind !== "ignored" ? <Button disabled={busy} variant="outline" onClick={() => void scan()}>{t("knowledge.files.rescan")}</Button> : null}
        </div>
        <Input aria-label={t("knowledge.files.filter")} value={query} onChange={(event) => { setQuery(event.target.value); setPage(1); }} placeholder={t("knowledge.files.filter")} />
        {busy ? <p role="status">{t(savingRules ? "knowledge.files.savingRules" : "knowledge.files.loading")}</p> : null}
        {tab === "files" ? <>
          <p className="text-sm" role="status">{t("knowledge.files.counts").replace("{selected}", String(selected.size)).replace("{ready}", String(eligible.length)).replace("{blocked}", String((preview?.entries.length ?? 0) - eligible.length))}</p>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="outline" disabled={busy || !preview} onClick={() => { eligible.forEach((row) => choices.current.set(row.key, true)); setSelected(new Set(eligible.map((row) => row.key))); }}>{t("knowledge.files.selectAll")}</Button>
            <Button size="sm" variant="outline" disabled={busy || !preview} onClick={() => { eligible.forEach((row) => choices.current.set(row.key, false)); setSelected(new Set()); }}>{t("knowledge.files.selectNone")}</Button>
            <Button size="sm" variant="outline" disabled={busy || !selected.size || !preview} onClick={() => void changeRules(eligible.filter((row) => selected.has(row.key)), true)}>{t("knowledge.files.ignoreSelected")}</Button>
          </div>
          <div className="overflow-x-auto rounded-md border">
            <table className="w-full text-left text-sm"><thead><tr className="border-b bg-muted/50"><th className="p-2">{t("knowledge.files.file")}</th><th className="p-2">{t("knowledge.files.size")}</th><th className="p-2">{t("knowledge.files.state")}</th><th className="p-2">{t("knowledge.files.actions")}</th></tr></thead>
              <tbody>{filtered.slice((currentPage - 1) * 100, currentPage * 100).map((row) => <tr key={row.key} className={`border-b last:border-0 ${row.disposition !== "ready" ? "bg-muted/30 text-muted-foreground" : ""}`}>
                <td className="min-w-52 max-w-md p-2"><label className="flex items-start gap-2"><input type="checkbox" className="mt-1 shrink-0" aria-label={row.relativePath} disabled={busy || row.disposition !== "ready"} checked={row.disposition === "ready" && selected.has(row.key)} onChange={(event) => { choices.current.set(row.key, event.target.checked); setSelected((current) => { const next = new Set(current); if (event.target.checked) next.add(row.key); else next.delete(row.key); return next; }); }} /><span className="break-all" title={row.sourceRoot}>{row.relativePath}<small className="block text-xs text-muted-foreground">{row.sourceRoot}</small></span></label></td>
                <td className="whitespace-nowrap p-2">{row.byteSize.toLocaleString()} B</td>
                <td className="max-w-64 p-2">{t(DISPOSITION_LABELS[row.disposition])}{row.detail ? <small className="block break-all">{row.detail}</small> : null}</td>
                <td className="p-2"><Button size="sm" variant="ghost" disabled={busy} onClick={() => void changeRules([row], row.disposition !== "ignored")}>{t(row.disposition === "ignored" ? "knowledge.files.unignore" : "knowledge.files.ignore")}</Button></td>
              </tr>)}</tbody>
            </table>
          </div>
        </> : <>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="outline" disabled={busy} onClick={() => setRemoval(new Set(ignoredRows.map(ignoreIdentity)))}>{t("knowledge.files.selectAllIgnored")}</Button>
            <Button size="sm" variant="outline" disabled={busy || !removal.size} onClick={() => void changeRules(rules.filter((row) => removal.has(ignoreIdentity(row))), false)}>{t("knowledge.files.unignoreSelected")}</Button>
          </div>
          <ul className="space-y-2">{ignoredRows.slice((currentPage - 1) * 100, currentPage * 100).map((row) => <li key={ignoreIdentity(row)} className="flex items-start gap-2 rounded-md border p-3 text-sm">
            <input type="checkbox" className="mt-1" aria-label={row.relativePath} disabled={busy} checked={removal.has(ignoreIdentity(row))} onChange={(event) => setRemoval((current) => { const next = new Set(current); if (event.target.checked) next.add(ignoreIdentity(row)); else next.delete(ignoreIdentity(row)); return next; })} />
            <span className="min-w-0 flex-1 break-all">{row.relativePath}<small className="block text-muted-foreground">{row.sourceRoot}</small></span>
            <Button size="sm" variant="outline" disabled={busy} onClick={() => void changeRules([row], false)}>{t("knowledge.files.unignore")}</Button>
          </li>)}</ul>
        </>}
        {!busy && count === 0 ? <p className="text-sm text-muted-foreground">{t("knowledge.files.empty")}</p> : null}
        {pageCount > 1 ? <div className="flex items-center justify-end gap-2"><Button size="sm" variant="outline" disabled={currentPage <= 1} onClick={() => setPage(currentPage - 1)}>{t("knowledge.files.previous")}</Button><span>{currentPage} / {pageCount}</span><Button size="sm" variant="outline" disabled={currentPage >= pageCount} onClick={() => setPage(currentPage + 1)}>{t("knowledge.files.next")}</Button></div> : null}
      </div>
      <DialogFooter className="m-0 shrink-0 border-t px-5 py-3">
        <Button variant="outline" disabled={savingRules} onClick={dismiss}>{t("knowledge.files.cancel")}</Button>
        {target.kind !== "ignored" ? <Button disabled={busy || !preview || Boolean(error)} onClick={() => { if (preview) onConfirm({ fingerprint: preview.fingerprint, selectedKeys: [...selected] }); }}>{t(target.kind === "import" ? "knowledge.files.confirmImport" : "knowledge.files.confirmBuild")}</Button> : null}
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}
