import * as React from "react";
import { BookMarked, Download, FileText, Gauge, History, Library, Link2, PencilLine, Plus, RefreshCw, Search, Settings2, Trash2, Upload } from "lucide-react";
import { PageEmptyState, PageNotice } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { Input } from "@/ui-kit/primitives/input";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/ui-kit/primitives/tabs";
import { AlertDialog, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog";
import { ProjectFolderPickerDialog } from "@/components/project/folder-picker-dialog";
import { browseProjectDirectories } from "@/service/project-actions-service";
import * as knowledge from "@/service/knowledge-base-service";
import { useI18n } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";
import type { KnowledgeLibrarySummary, KnowledgePackageInspection } from "@/types/knowledge-base";
import { KnowledgeLibraryCreateDialog } from "./knowledge-library-create-dialog";
import { KnowledgeMetadataDialog } from "./knowledge-metadata-dialog";
import { KnowledgePackageImportDialog, KnowledgePackagePickerDialog } from "./knowledge-package-dialogs";
import { KnowledgeMessageDialog, KnowledgeReadFailure } from "./knowledge-feedback";
import { KnowledgeContent } from "./knowledge-content";
import { KnowledgeHistory } from "./knowledge-history";
import { KnowledgeIndex } from "./knowledge-index";
import { KnowledgeBindings, KnowledgeDictionary } from "./knowledge-management";
import { KnowledgeLibraryCard, KnowledgeLibraryHero, knowledgeLibraryReadable } from "./knowledge-library-overview";
import { useKnowledgeRead } from "./use-knowledge-read";
import "@/ui-kit/styles/features/knowledge.css";

// Human lifecycle management receives projects from their authoritative workbench owner.
// 人工生命周期管理从权威工作台所有者接收项目。
interface KnowledgeBaseAreaProps {
  // Current projects available for explicit library authorization.
  // 当前可供明确知识库授权的项目。
  projects: WorkbenchSnapshot["projects"];
}

// Creation and mode editing freeze separate explicit owners.
// 创建及模式编辑冻结分别明确的所有者。
type KnowledgePolicyDialog = { kind: "create" } | { kind: "mode"; library: KnowledgeLibrarySummary };

// Manages library lifecycle while all document and folder content stays read-only.
// 管理知识库生命周期，同时全部文档及目录内容保持只读。
// Parameters: projects is the authoritative workbench list. Returns creation, configuration, history, index and package controls.
// 参数：projects 为权威工作台列表。返回创建、配置、历史、索引及套件控件。
export function KnowledgeBaseArea({ projects }: KnowledgeBaseAreaProps) {
  // All visible manager copy comes from shared catalogs.
  // 全部可见管理文案来自共享目录。
  const { t, locale } = useI18n();
  // Registry failures remain distinct from a valid empty library collection.
  // 注册表失败与有效空知识库集合保持不同。
  const load = React.useCallback(() => knowledge.listKnowledgeLibraries(), []);
  // A revocable read lifetime rejects late registry responses.
  // 可撤销读取寿命拒绝迟到注册表响应。
  const registry = useKnowledgeRead("knowledge-registry", load);
  // Selection is an explicit stable ID, never a name or positional fallback.
  // 选择为明确稳定身份，绝非名称或位置兜底。
  const [selectedId, setSelectedId] = React.useState<string | null>(null);
  // Filtering changes only this view.
  // 筛选仅改变此视图。
  const [filter, setFilter] = React.useState("");
  // One parent serializes lifecycle mutations across panels.
  // 一个父级串行化各面板生命周期变更。
  const [busy, setBusy] = React.useState(false);
  // Synchronous ownership closes the double-click interval.
  // 同步归属封闭双击间隔。
  const mutation = React.useRef(false);
  // Late responses cannot publish after this manager unmounts.
  // 此管理器卸载后迟到响应不能发布。
  const active = React.useRef(true);
  React.useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  // Dialog owners are frozen at the explicit user action.
  // 弹窗所有者在明确用户动作时冻结。
  const [policy, setPolicy] = React.useState<KnowledgePolicyDialog | null>(null);
  // Metadata editing never receives document content.
  // 元信息编辑绝不接收文档内容。
  const [metadata, setMetadata] = React.useState<KnowledgeLibrarySummary | null>(null);
  // Destructive confirmation names one immutable owner.
  // 破坏性确认指定一个不可变所有者。
  const [deleting, setDeleting] = React.useState<KnowledgeLibrarySummary | null>(null);
  // Failed deletion retains the confirmation and diagnosis.
  // 删除失败保留确认及诊断。
  const [deleteError, setDeleteError] = React.useState<string | null>(null);
  // Inspection precedes separate import confirmation.
  // 检查先于独立导入确认。
  const [pickingPackage, setPickingPackage] = React.useState(false);
  // Import binds the complete inspected byte fingerprint.
  // 导入绑定完整已检查字节指纹。
  const [inspection, setInspection] = React.useState<KnowledgePackageInspection | null>(null);
  // Export retains its chosen library during directory selection.
  // 目录选择期间导出保留选定库。
  const [exporting, setExporting] = React.useState<KnowledgeLibrarySummary | null>(null);
  // Feedback never offers replay of a committed mutation.
  // 反馈绝不提供已提交变更的重放。
  const [message, setMessage] = React.useState<string | null>(null);
  // Only a successful current registry permits lifecycle writes.
  // 仅成功当前注册表允许生命周期写入。
  const writable = registry.value !== null && registry.error === null && !registry.loading && !busy;
  // A selected row can disappear after deletion or failed refresh.
  // 删除或刷新失败后选择行可消失。
  const selected = registry.value === null ? undefined : registry.value.find((library) => library.id === selectedId);
  // Local matching never triggers inference.
  // 本地匹配绝不触发推理。
  const visible = registry.value === null ? [] : registry.value.filter((library) => (library.name + "\n" + library.description).toLocaleLowerCase().includes(filter.toLocaleLowerCase()));

  // Serializes a mutation and attempts one registry refresh after commit.
  // 串行化变更，并在提交后尝试一次注册表刷新。
  // Parameters: action commits one exact owner. Returns after commit/refresh; only mutation failures reject.
  // 参数：action 提交一个精确所有者。提交及刷新后返回；仅变更失败拒绝。
  async function runMutation(action: () => Promise<void>): Promise<void> {
    if (mutation.current || !writable) throw new Error("Knowledge management is busy or its registry is unavailable.");
    mutation.current = true;
    setBusy(true);
    try {
      await action();
      if (active.current && !await registry.refresh(true)) setMessage(t("knowledge.changedRefreshFailed"));
    } finally {
      mutation.current = false;
      if (active.current) setBusy(false);
    }
  }

  // Deletes the frozen owner and preserves failures in its confirmation.
  // 删除冻结所有者，并在其确认中保留失败。
  // Parameters: none. Returns after the deletion and refresh attempt.
  // 参数：无。删除及刷新尝试后返回。
  async function confirmDelete(): Promise<void> {
    if (deleting === null || mutation.current) return;
    setDeleteError(null);
    try {
      await runMutation(async () => { await knowledge.deleteKnowledgeLibrary(deleting.id); });
      if (active.current) { setDeleting(null); if (selectedId === deleting.id) setSelectedId(null); }
    } catch (reason) { if (active.current) setDeleteError(reason instanceof Error ? reason.message : String(reason)); }
  }

  return <div className="knowledge-page vk-bleed">
    <header className="knowledge-page-toolbar">
      <div className="knowledge-search-field"><Search size={14} aria-hidden="true" /><Input aria-label={t("knowledge.filter")} placeholder={t("knowledge.filter")} value={filter} disabled={registry.value === null} onChange={(event) => setFilter(event.target.value)} /></div>
      <div className="knowledge-actions">
        <Button size="sm" variant="ghost" disabled={busy || registry.loading} onClick={() => void registry.refresh(true)}><RefreshCw className={registry.loading ? "vk-spin" : undefined} size={15} />{t("skills.action.refresh")}</Button>
        <Button size="sm" variant="outline" disabled={!writable} onClick={() => setPickingPackage(true)}><Upload size={15} />{t("knowledge.package.import")}</Button>
        <Button size="sm" disabled={!writable} onClick={() => setPolicy({ kind: "create" })}><Plus size={15} />{t("knowledge.create")}</Button>
      </div>
    </header>
    {registry.error !== null ? <KnowledgeReadFailure error={registry.error} busy={busy || registry.loading} onRetry={registry.refresh} /> : null}
    {registry.loading && registry.value === null ? <StreamPlaceholder label={t("knowledge.librariesLoading")} /> : null}
    {registry.value !== null ? <div className="knowledge-workspace">
      <aside className="knowledge-rail">
        {registry.value.length === 0 ? <PageEmptyState icon={<Library />} title={t("knowledge.empty")} /> : null}
        {registry.value.length > 0 && visible.length === 0 ? <p className="knowledge-rail-note">{t("knowledge.filterEmpty")}</p> : null}
        <ul className="knowledge-rail-list">{visible.map((library) => <li key={library.id}><KnowledgeLibraryCard library={library} t={t} selected={library.id === selectedId} disabled={busy} onSelect={() => setSelectedId(library.id)} /></li>)}</ul>
      </aside>
      {selected === undefined ? <section className="knowledge-detail knowledge-detail-placeholder"><PageEmptyState icon={<Library />} title={t("knowledge.selectLibrary")} description={t("knowledge.content.readOnly")} /></section> : <section className="knowledge-detail">
        <KnowledgeLibraryHero library={selected} t={t} locale={locale} actions={<>
          {knowledgeLibraryReadable(selected) ? <><Button size="sm" variant="outline" disabled={!writable} onClick={() => setMetadata(selected)}><PencilLine size={14} />{t("knowledge.metadata")}</Button><Button size="sm" variant="outline" disabled={!writable} onClick={() => setPolicy({ kind: "mode", library: selected })}><Settings2 size={14} />{t("knowledge.indexMode")}</Button><Button size="sm" variant="outline" disabled={!writable} onClick={() => setExporting(selected)}><Download size={14} />{t("knowledge.package.export")}</Button></> : null}
          <Button size="sm" variant="outline" disabled={!writable} onClick={() => { setDeleteError(null); setDeleting(selected); }}><Trash2 size={14} />{t("knowledge.delete")}</Button>
        </>} />
        {!knowledgeLibraryReadable(selected) ? <div className="knowledge-tabs"><PageNotice tone="error" title={t("knowledge.unavailableDeleteOnly")} details={selected.errorMessage ?? undefined} detailsLabel={t("toolPresentation.output.errorDetails")} /></div> :
          <Tabs key={selected.id} defaultValue="content" className="knowledge-tabs">
            <TabsList><TabsTrigger value="content"><FileText size={14} />{t("knowledge.tabs.content")}</TabsTrigger><TabsTrigger value="history"><History size={14} />{t("knowledge.tabs.history")}</TabsTrigger><TabsTrigger value="index"><Gauge size={14} />{t("knowledge.tabs.index")}</TabsTrigger><TabsTrigger value="bindings"><Link2 size={14} />{t("knowledge.tabs.bindings")}</TabsTrigger><TabsTrigger value="dictionary"><BookMarked size={14} />{t("knowledge.tabs.dictionary")}</TabsTrigger></TabsList>
            <TabsContent value="content" className="knowledge-tab-panel"><KnowledgeContent library={selected} /></TabsContent>
            <TabsContent value="history" className="knowledge-tab-panel"><KnowledgeHistory library={selected} busy={!writable} onRestore={(targetSeq) => runMutation(async () => { await knowledge.restoreKnowledgeLibrary({ libraryId: selected.id, targetSeq, note: "" }); })} /></TabsContent>
            <TabsContent value="index" className="knowledge-tab-panel"><KnowledgeIndex library={selected} busy={!writable}
              onRebuild={() => runMutation(async () => { await knowledge.rebuildKnowledgeIndex(selected.id); })}
              onRepair={() => runMutation(async () => { await knowledge.repairKnowledgeIndex(selected.id); })}
              onCancel={(operationId) => runMutation(async () => { await knowledge.cancelKnowledgeOperation({ libraryId: selected.id, operationId }); })}
              onRetry={(documentIds) => runMutation(async () => { await knowledge.retryKnowledgeEmbeddings({ libraryId: selected.id, documentIds }); })} /></TabsContent>
            <TabsContent value="bindings" className="knowledge-tab-panel"><KnowledgeBindings libraryId={selected.id} projects={projects} busy={!writable} onSave={(request) => runMutation(async () => { await knowledge.setKnowledgeProjectBinding(request); knowledge.notifyKnowledgeProjectBindingsChanged(request.projectId); })} /></TabsContent>
            <TabsContent value="dictionary" className="knowledge-tab-panel"><KnowledgeDictionary libraryId={selected.id} busy={!writable} onSave={(word, weight) => runMutation(async () => { await knowledge.upsertKnowledgeCustomWord({ libraryId: selected.id, word, weight }); })} onRemove={(word) => runMutation(async () => { await knowledge.removeKnowledgeCustomWord(selected.id, word); })} /></TabsContent>
          </Tabs>}
      </section>}
    </div> : null}
    {policy !== null ? <KnowledgeLibraryCreateDialog library={policy.kind === "create" ? null : policy.library} busy={!writable} onClose={() => setPolicy(null)} onSubmit={(request) => runMutation(async () => {
      if (policy.kind === "create") {
        // Only the service-created identity may become the new selection.
        // 仅服务创建身份可成为新选择。
        const created = await knowledge.createKnowledgeLibrary(request);
        if (active.current) setSelectedId(created.id);
      } else await knowledge.setKnowledgeIndexMode({ libraryId: policy.library.id, settings: request.settings });
    })} /> : null}
    {metadata !== null ? <KnowledgeMetadataDialog library={metadata} busy={!writable} onClose={() => setMetadata(null)} onSave={(request) => runMutation(async () => { await knowledge.renameKnowledgeLibrary(request); })} /> : null}
    {pickingPackage ? <KnowledgePackagePickerDialog open onOpenChange={setPickingPackage} onSelect={async (archivePath) => {
      // The inspection is bound to this exact host file.
      // 检查绑定此精确宿主文件。
      const checked = await knowledge.inspectKnowledgePackage({ archivePath });
      if (active.current) setInspection(checked);
    }} /> : null}
    {inspection !== null && !pickingPackage ? <KnowledgePackageImportDialog inspection={inspection} busy={!writable} onClose={() => setInspection(null)} onImport={(name) => runMutation(async () => {
      // Import revalidates the inspected fingerprint before creating an independent library.
      // 导入在创建独立库前重新验证已检查指纹。
      const imported = await knowledge.importKnowledgePackage({ archivePath: inspection.archivePath, expectedSha256: inspection.sha256, name });
      if (active.current) setSelectedId(imported.library.id);
    })} /> : null}
    {exporting !== null ? <ProjectFolderPickerDialog open title={t("knowledge.package.exportPickerTitle")} confirmLabel={t("knowledge.package.export")} onBrowse={browseProjectDirectories} onOpenChange={(open) => { if (!open && !mutation.current) setExporting(null); }} onConfirm={(destinationDirectory) => runMutation(async () => {
      // Export reads the chosen library and returns the exact archive created by the host.
      // 导出读取选定库并返回宿主创建的精确归档。
      const exported = await knowledge.exportKnowledgePackage({ libraryId: exporting.id, destinationDirectory });
      if (active.current) { setExporting(null); setMessage(t("knowledge.package.exported").replace("{path}", exported.archivePath)); }
    })} /> : null}
    <AlertDialog open={deleting !== null} onOpenChange={(open) => { if (!open && !busy) setDeleting(null); }}><AlertDialogContent><AlertDialogHeader><AlertDialogTitle>{t("knowledge.delete")}</AlertDialogTitle><AlertDialogDescription>{t("knowledge.deleteConfirm").replace("{name}", deleting === null ? "" : deleting.name)}</AlertDialogDescription></AlertDialogHeader>{deleteError !== null ? <pre role="alert" className="vk-diag">{deleteError}</pre> : null}<AlertDialogFooter><Button variant="outline" disabled={busy} onClick={() => setDeleting(null)}>{t("conversation.action.cancel")}</Button><Button variant="destructive" disabled={!writable} onClick={() => void confirmDelete()}>{t("knowledge.delete")}</Button></AlertDialogFooter></AlertDialogContent></AlertDialog>
    <KnowledgeMessageDialog message={message} onClose={() => setMessage(null)} />
  </div>;
}
