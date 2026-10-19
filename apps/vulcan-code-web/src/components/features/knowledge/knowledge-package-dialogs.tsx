import * as React from "react";
import { InlineStreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { ArrowUp, Check, ChevronRight, FileArchive, Folder, FolderOpen, HardDrive, RefreshCw } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Input } from "@/ui-kit/primitives/input";
import { Spinner } from "@/ui-kit/primitives/spinner";
import { useI18n } from "@/i18n";
import { browseKnowledgePackages } from "@/service/knowledge-base-service";
import type { KnowledgePackageFileListing, KnowledgePackageInspection } from "@/types/knowledge-base";
import { formatBytes } from "@/lib/display-format";
import { KNOWLEDGE_MODE_COPY } from "./knowledge-mode";
import "@/ui-kit/styles/features/project-folder-picker.css";

// Parameters: path is a native directory identity. Returns its comparison key for the host-declared root list.
// 参数：path 为原生目录身份。返回其在宿主声明根列表中的比较键。
function knowledgePackagePathKey(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/g, "").toLocaleLowerCase() || "/";
}

// Parameters: props owns picker visibility and exact-file inspection. Returns host directory navigation without mutating content.
// 参数：props 拥有选择器可见性及精确文件检查。返回宿主目录导航，不变更内容。
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
    // Navigation revokes the previous directory before a failed read can expose selectable stale files.
    // 导航先撤销先前目录，避免失败读取暴露可选旧文件。
    setListing(null);
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
          <Button aria-label={t("projectFolderPicker.openPath")} disabled={loading || submitting} size="icon" type="submit" variant="outline">{loading ? <Spinner /> : <ChevronRight size={16} />}</Button>
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
          <Button disabled={!selectedPath || submitting || loading} onClick={() => void handleConfirm()} type="button">{submitting ? <Spinner /> : <Check size={16} />}{submitting ? t("knowledge.package.inspecting") : t("knowledge.package.inspect")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
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
