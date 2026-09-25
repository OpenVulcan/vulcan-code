import * as React from "react";
import { AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction } from "@/components/ui/alert-dialog";
import { InlineStreamPlaceholder } from "@/components/ui/stream-placeholder";
import { Archive, ArrowUp, Check, ChevronRight, FileArchive, Folder, FolderOpen, HardDrive, RefreshCw, TriangleAlert } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useI18n } from "@/i18n";
import { browseKnowledgePackages } from "@/service/knowledge-base-service";
import type { KnowledgeLibrarySummary, KnowledgePackageFileListing, KnowledgePackageInspection } from "@/types";
import { formatBytes } from "@/lib/display-format";

// knowledgePackagePathKey normalizes a native path for root-selection matching.
// knowledgePackagePathKey 规范化原生路径以匹配根目录选择状态。
function knowledgePackagePathKey(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/g, "").toLocaleLowerCase() || "/";
}

// findKnowledgePackageNameConflict resolves the exact existing row matching one normalized candidate name.
// findKnowledgePackageNameConflict 解析与规范化候选名称精确匹配的现有知识库行。
export function findKnowledgePackageNameConflict(
  libraries: KnowledgeLibrarySummary[],
  name: string,
): KnowledgeLibrarySummary | null {
  // normalizedName mirrors the backend's trim plus locale-independent lowercase comparison key.
  // normalizedName 对齐后端“去除首尾空白并执行非区域化小写”的比较键。
  const normalizedName = name.trim().toLowerCase();
  if (!normalizedName) return null;
  return libraries.find((library) => library.name.trim().toLowerCase() === normalizedName) ?? null;
}

// KnowledgePackagePickerDialog lets the user navigate host directories and select one ordinary .vkb file.
// KnowledgePackagePickerDialog 允许用户浏览宿主目录并选择一个普通 .vkb 文件。
// Parameters: props supplies open state and package-check callbacks; returns host directory navigation, an exact file selection and guarded actions.
// 参数：props 提供打开状态及知识库包检查回调；返回宿主目录导航、精确文件选择及受保护操作。
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
  // t resolves user-visible package-picker copy.
  // t 解析用户可见的知识库包选择器文案。
  const { t } = useI18n();
  // listing stores the current host directory and its selectable packages.
  // listing 保存当前宿主目录及其可选知识库包。
  const [listing, setListing] = React.useState<KnowledgePackageFileListing | null>(null);
  // pathInput stores the editable native directory path.
  // pathInput 保存可编辑的原生目录路径。
  const [pathInput, setPathInput] = React.useState("");
  // selectedPath identifies the exact package selected in the current directory.
  // selectedPath 标识当前目录中精确选中的知识库包。
  const [selectedPath, setSelectedPath] = React.useState<string | null>(null);
  // loading tracks directory navigation requests.
  // loading 跟踪目录导航请求。
  const [loading, setLoading] = React.useState(false);
  // submitting tracks package inspection and prevents duplicate confirmation.
  // submitting 跟踪知识库包检查并防止重复确认。
  const [submitting, setSubmitting] = React.useState(false);
  // submittingRef synchronously prevents duplicate package inspection before React rerenders.
  // submittingRef 在 React 重新渲染前同步阻止重复知识库包检查。
  const submittingRef = React.useRef(false);
  // error stores the latest local navigation or inspection failure.
  // error 保存最近一次本地导航或检查失败。
  const [error, setError] = React.useState<string | null>(null);
  // browseSequenceRef rejects directory responses from an obsolete navigation request.
  // browseSequenceRef 丢弃过期导航请求返回的目录响应。
  const browseSequenceRef = React.useRef(0);
  // roots exposes the current service-defined filesystem roots.
  // roots 暴露当前由服务定义的文件系统根目录。
  const roots = React.useMemo(() => listing?.directory.roots ?? [], [listing?.directory.roots]);
  // activeRootPath resolves the longest root prefix owning the current directory.
  // activeRootPath 解析包含当前目录的最长根路径前缀。
  const activeRootPath = React.useMemo(() => {
    if (!listing) return null;
    const currentKey = knowledgePackagePathKey(listing.directory.path);
    let matchedPath: string | null = null;
    let matchedLength = -1;
    roots.forEach((root) => {
      const rootKey = knowledgePackagePathKey(root.path);
      const matches = rootKey === "/" ? currentKey.startsWith("/") : currentKey === rootKey || currentKey.startsWith(`${rootKey}/`);
      if (matches && rootKey.length > matchedLength) {
        matchedPath = root.path;
        matchedLength = rootKey.length;
      }
    });
    return matchedPath;
  }, [listing, roots]);

  // loadDirectory requests one directory and discards stale browser responses.
  // loadDirectory 请求一个目录并丢弃过期浏览响应。
  const loadDirectory = React.useCallback(async (path: string | null) => {
    // sequence gives this navigation request a unique freshness boundary.
    // sequence 为本次导航请求提供唯一的新鲜度边界。
    const sequence = browseSequenceRef.current + 1;
    browseSequenceRef.current = sequence;
    setLoading(true);
    setError(null);
    setSelectedPath(null);
    try {
      // nextListing is the authoritative response for the requested directory.
      // nextListing 是请求目录的权威响应。
      const nextListing = await browseKnowledgePackages(path);
      if (browseSequenceRef.current !== sequence) return;
      setListing(nextListing);
      setPathInput(nextListing.directory.path);
    } catch (nextError) {
      if (browseSequenceRef.current === sequence) {
        setError(nextError instanceof Error ? nextError.message : String(nextError));
      }
    } finally {
      if (browseSequenceRef.current === sequence) setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    if (props.open) void loadDirectory(null);
    return () => { browseSequenceRef.current += 1; };
  }, [loadDirectory, props.open]);

  // handlePathSubmit navigates to the manually entered directory.
  // handlePathSubmit 导航到手工输入的目录。
  function handlePathSubmit(event: React.FormEvent): void {
    event.preventDefault();
    void loadDirectory(pathInput.trim() || null);
  }

  // handleConfirm inspects the selected archive before closing the picker.
  // handleConfirm 在关闭选择器前检查选中的归档。
  async function handleConfirm(): Promise<void> {
    if (!selectedPath || submittingRef.current) return;
    submittingRef.current = true;
    setSubmitting(true);
    setError(null);
    try {
      await props.onSelect(selectedPath);
      props.onOpenChange(false);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={props.open} onOpenChange={(open) => !submittingRef.current && props.onOpenChange(open)}>
      <DialogContent className="project-folder-picker-dialog knowledge-package-picker-dialog" showCloseButton={!submitting}>
        <DialogHeader><DialogTitle>{t("knowledge.package.importPickerTitle")}</DialogTitle></DialogHeader>
        <form className="project-folder-picker-path-row" onSubmit={handlePathSubmit}>
          <Input aria-label={t("projectFolderPicker.pathAria")} disabled={loading || submitting} onChange={(event) => setPathInput(event.target.value)} value={pathInput} />
          <Button aria-label={t("projectFolderPicker.openPath")} disabled={loading || submitting} size="icon" type="submit" variant="outline">{loading ? <Spinner className="size-4" /> : <ChevronRight size={16} />}</Button>
        </form>
        <div className="project-folder-picker-toolbar">
          <Button disabled={!listing?.directory.parentPath || loading || submitting} onClick={() => void loadDirectory(listing?.directory.parentPath ?? null)} size="sm" type="button" variant="outline"><ArrowUp size={15} />{t("projectFolderPicker.parent")}</Button>
          <Button disabled={!listing || loading || submitting} onClick={() => void loadDirectory(listing?.directory.path ?? null)} size="sm" type="button" variant="outline"><RefreshCw size={15} />{t("skills.action.refresh")}</Button>
        </div>
        <div className="project-folder-picker-browser">
          <aside aria-label={t("projectFolderPicker.rootsAria")} className="project-folder-picker-roots">
            {roots.map((root) => (
              <button className={root.path === activeRootPath ? "project-folder-picker-root active" : "project-folder-picker-root"} disabled={loading || submitting} key={root.path} onClick={() => void loadDirectory(root.path)} type="button"><HardDrive size={16} /><span>{root.label}</span></button>
            ))}
          </aside>
          <section className="project-folder-picker-directory">
            <div className="project-folder-picker-current"><FolderOpen size={16} /><span>{listing?.directory.path ?? <InlineStreamPlaceholder label={t("workspaceFiles.loading")} />}</span></div>
            <div className="project-folder-picker-list" role="list">
              {listing?.directory.entries.map((entry) => (
                <button className="project-folder-picker-row" disabled={loading || submitting} key={entry.path} onClick={() => void loadDirectory(entry.path)} type="button"><Folder size={16} /><span>{entry.name}</span></button>
              ))}
              {listing?.packages.map((file) => (
                <button className={`project-folder-picker-row knowledge-package-file${selectedPath === file.path ? " selected" : ""}`} disabled={loading || submitting} key={file.path} onClick={() => setSelectedPath(file.path)} type="button"><FileArchive size={16} /><span>{file.name}</span><small>{formatBytes(file.byteSize)}</small></button>
              ))}
              {listing && listing.directory.entries.length === 0 && listing.packages.length === 0 ? <div className="project-folder-picker-empty">{t("knowledge.package.noPackages")}</div> : null}
            </div>
          </section>
        </div>
        {error ? <div className="project-folder-picker-error">{error}</div> : null}
        <DialogFooter>
          <Button disabled={submitting} onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("conversation.action.cancel")}</Button>
          <Button disabled={!selectedPath || submitting || loading} onClick={() => void handleConfirm()} type="button">{submitting ? <Spinner className="size-4" /> : <Check size={16} />}{submitting ? t("knowledge.package.inspecting") : t("knowledge.package.inspect")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// KnowledgePackageImportDialog confirms a unique-name import or one explicit replacement target.
// KnowledgePackageImportDialog 确认唯一名称导入或一个显式覆盖目标。
// Parameters: props supplies package preview, existing libraries and import callbacks; returns local draft fields and exact-target replacement confirmation.
// 参数：props 提供包预览、现有资料库及导入回调；返回本地草稿字段与精确目标覆盖确认。
export function KnowledgePackageImportDialog(props: {
  // Validated package inspection currently awaiting confirmation.
  // 当前等待确认的已校验知识库包检查结果。
  inspection: KnowledgePackageInspection | null;
  // Authoritative existing libraries used for live name-conflict detection.
  // 用于实时名称冲突检测的权威现有知识库列表。
  libraries: KnowledgeLibrarySummary[];
  // Whether an import mutation is active.
  // 是否存在活动导入变更。
  busy: boolean;
  // Handles dialog visibility changes.
  // 处理弹窗可见性变化。
  onOpenChange: (open: boolean) => void;
  // Executes one confirmed import strategy.
  // 执行一个已确认的导入策略。
  onImport: (name: string, strategy: "create" | "replace", targetLibraryId: string | null) => Promise<void>;
}) {
  // t resolves user-visible import-decision copy.
  // t 解析用户可见的导入决策文案。
  const { t } = useI18n();
  // name stores the proposed target library name.
  // name 保存建议使用的目标知识库名称。
  const [name, setName] = React.useState("");
  // strategy stores the explicit non-overwriting or replacement decision.
  // strategy 保存显式的不覆盖新建或覆盖决策。
  const [strategy, setStrategy] = React.useState<"create" | "replace">("create");
  // replacementTarget freezes the exact library reviewed by the second destructive decision.
  // replacementTarget 冻结第二次破坏性决策已审阅的精确知识库。
  const [replacementTarget, setReplacementTarget] = React.useState<KnowledgeLibrarySummary | null>(null);
  // error stores a failed import request while preserving the user's decision.
  // error 保存失败的导入请求，同时保留用户决策。
  const [error, setError] = React.useState<string | null>(null);
  // submitting provides immediate visual feedback before the parent busy projection rerenders.
  // submitting 在父级忙碌投影重新渲染前提供即时视觉反馈。
  const [submitting, setSubmitting] = React.useState(false);
  // submittingRef synchronously serializes destructive confirmation and package import.
  // submittingRef 同步串行化破坏性确认与知识库包导入。
  const submittingRef = React.useRef(false);

  React.useEffect(() => {
    setName(props.inspection?.package.name ?? "");
    setStrategy("create");
    setError(null);
  }, [props.inspection?.archivePath, props.inspection?.package.name]);

  // conflict is the exact current registry row matching the edited name.
  // conflict 是与编辑后名称匹配的精确当前注册记录。
  const conflict = findKnowledgePackageNameConflict(props.libraries, name);
  // createBlocked prevents implicit overwrite and empty-name submission.
  // createBlocked 阻止隐式覆盖与空名称提交。
  const createBlocked = !name.trim() || Boolean(conflict);
  // effectiveBusy merges the local synchronous submission with the parent mutation state.
  // effectiveBusy 合并本地同步提交状态与父级变更状态。
  const effectiveBusy = props.busy || submitting;

  // handleImport enforces the explicit replacement confirmation before any mutation request.
  // handleImport 在发送任何变更请求前强制执行显式覆盖确认。
  // Parameters: replacementConfirmed proves the selected conflict was explicitly confirmed; returns a promise settling after validation and the guarded import attempt.
  // 参数：replacementConfirmed 证明选中冲突已经明确确认；返回在校验和受保护导入尝试完成后结束的 Promise。
  async function handleImport(replacementConfirmed = false): Promise<void> {
    if (!props.inspection || props.busy || submittingRef.current) return;
    if (strategy === "create" && createBlocked) return;
    if (strategy === "replace") {
      if (!conflict) return;
      if (!replacementConfirmed) { setReplacementTarget(conflict); return; }
      if (conflict.id !== replacementTarget?.id) { setError(t("knowledge.experience.targetChanged")); return; }
    }
    submittingRef.current = true;
    setSubmitting(true);
    setError(null);
    try {
      await props.onImport(name.trim(), strategy, strategy === "replace" ? conflict?.id ?? null : null);
      setReplacementTarget(null);
      props.onOpenChange(false);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  }

  // packageInfo is the validated immutable metadata displayed for the decision.
  // packageInfo 是决策界面展示的已校验不可变元数据。
  const packageInfo = props.inspection?.package;
  return (
    <Dialog open={props.inspection !== null} onOpenChange={(open) => !submittingRef.current && !props.busy && props.onOpenChange(open)}>
      <DialogContent className="knowledge-package-import-dialog" showCloseButton={!effectiveBusy}>
        <DialogHeader><DialogTitle>{t("knowledge.package.importTitle")}</DialogTitle></DialogHeader>
        {packageInfo ? (
          <div className="knowledge-package-import-body">
            <div className="knowledge-package-summary">
              <Archive size={22} />
              <div><strong>{packageInfo.name}</strong><span>{t("knowledge.package.counts").replace("{documents}", String(packageInfo.documentCount)).replace("{chunks}", String(packageInfo.chunkCount)).replace("{bytes}", formatBytes(packageInfo.contentBytes))}</span></div>
            </div>
            <label className="knowledge-package-name"><span>{t("knowledge.name")}</span><Input disabled={effectiveBusy || strategy === "replace"} onChange={(event) => {
              setName(event.target.value);
              setStrategy("create");
            }} value={strategy === "replace" && conflict ? conflict.name : name} /></label>
            {conflict ? <div className="knowledge-package-conflict"><TriangleAlert size={18} /><span>{t("knowledge.package.nameConflict").replace("{name}", conflict.name)}</span></div> : null}
            <div className="knowledge-package-strategies">
              <label className={strategy === "create" ? "selected" : ""}>
                <input checked={strategy === "create"} disabled={effectiveBusy} name="knowledge-package-strategy" onChange={() => setStrategy("create")} type="radio" />
                <span><strong>{t("knowledge.package.createStrategy")}</strong><small>{conflict ? t("knowledge.package.renameRequired") : t("knowledge.package.createDescription")}</small></span>
              </label>
              {conflict ? (
                <label className={strategy === "replace" ? "selected danger" : "danger"}>
                  <input checked={strategy === "replace"} disabled={effectiveBusy} name="knowledge-package-strategy" onChange={() => setStrategy("replace")} type="radio" />
                  <span><strong>{t("knowledge.package.replaceStrategy")}</strong><small>{t("knowledge.package.replaceDescription")}</small></span>
                </label>
              ) : null}
            </div>
          </div>
        ) : null}
        {error ? <div className="project-folder-picker-error">{error}</div> : null}
        <DialogFooter>
          <Button disabled={effectiveBusy} onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("conversation.action.cancel")}</Button>
          <Button disabled={effectiveBusy || (strategy === "create" ? createBlocked : !conflict)} onClick={() => void handleImport()} type="button" variant={strategy === "replace" ? "destructive" : "default"}>{effectiveBusy ? <Spinner className="size-4" /> : <Check size={16} />}{strategy === "replace" ? t("knowledge.package.replaceStrategy") : t("knowledge.package.import")}</Button>
        </DialogFooter>
      </DialogContent>
      <AlertDialog open={replacementTarget !== null} onOpenChange={(open) => { if (!open && !submittingRef.current) setReplacementTarget(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("knowledge.package.replaceStrategy")}</AlertDialogTitle>
            <AlertDialogDescription>{t("knowledge.package.replaceConfirm")
              .replace("{name}", replacementTarget?.name ?? "")
              .replace("{documents}", String(replacementTarget?.documentCount ?? 0))
              .replace("{chunks}", String(replacementTarget?.chunkCount ?? 0))}</AlertDialogDescription>
          </AlertDialogHeader>
          {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={effectiveBusy}>{t("conversation.action.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={effectiveBusy || !conflict} onClick={(event) => { event.preventDefault(); void handleImport(true); }} variant="destructive">{t("knowledge.package.replaceStrategy")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Dialog>
  );
}
