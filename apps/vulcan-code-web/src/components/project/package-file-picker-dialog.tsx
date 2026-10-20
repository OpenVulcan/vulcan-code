import * as React from "react";
import { InlineStreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { ArrowUp, Check, ChevronRight, FileArchive, Folder, FolderOpen, HardDrive, RefreshCw } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Input } from "@/ui-kit/primitives/input";
import { Spinner } from "@/ui-kit/primitives/spinner";
import { useI18n } from "@/i18n";
import type { PackageFileListing } from "@/types";
import { formatBytes } from "@/lib/display-format";
import "@/ui-kit/styles/features/project-folder-picker.css";

// Parameters: path is a native directory identity. Returns its comparison key for the host-declared root list.
// 参数：path 为原生目录身份。返回其在宿主声明根列表中的比较键。
function packagePathKey(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/g, "").toLocaleLowerCase() || "/";
}

// PackageFilePickerDialogProps configures a picker for one kind of package file.
// PackageFilePickerDialogProps 配置某一类套件文件的选择器。
export interface PackageFilePickerDialogProps {
  // Whether the package picker is visible.
  // 套件选择器是否可见。
  open: boolean;
  // Handles picker visibility changes.
  // 处理选择器可见性变化。
  onOpenChange: (open: boolean) => void;
  // Loads one directory with its package files from the Core Service.
  // 从 Core Service 加载单个目录及其套件文件。
  onBrowse: (path: string | null) => Promise<PackageFileListing>;
  // Handles the exact package selected by the user; a rejection stays in the picker.
  // 处理用户选择的精确套件；失败时停留在选择器中。
  onSelect: (path: string) => Promise<void>;
  // Dialog title.
  // 对话框标题。
  title: string;
  // Text shown when the directory holds neither folders nor packages.
  // 目录既无子目录也无套件时显示的文字。
  emptyLabel: string;
  // Confirmation button label.
  // 确认按钮文案。
  confirmLabel: string;
  // Confirmation button label while onSelect runs.
  // onSelect 运行时的确认按钮文案。
  busyLabel: string;
}

// Parameters: props owns picker visibility and exact-file handling. Returns host directory navigation without mutating content.
// 参数：props 拥有选择器可见性及精确文件处理。返回宿主目录导航，不变更内容。
export function PackageFilePickerDialog(props: PackageFilePickerDialogProps) {
  // t resolves user-visible package-picker copy.
  // t 解析用户可见的套件选择器文案。
  const { t } = useI18n();
  const { onBrowse } = props;
  // listing stores the current host directory and its selectable packages.
  // listing 保存当前宿主目录及其可选套件。
  const [listing, setListing] = React.useState<PackageFileListing | null>(null);
  // pathInput stores the editable native directory path.
  // pathInput 保存可编辑的原生目录路径。
  const [pathInput, setPathInput] = React.useState("");
  // selectedPath identifies the exact package selected in the current directory.
  // selectedPath 标识当前目录中精确选中的套件。
  const [selectedPath, setSelectedPath] = React.useState<string | null>(null);
  // loading tracks directory navigation requests.
  // loading 跟踪目录导航请求。
  const [loading, setLoading] = React.useState(false);
  // submitting tracks package handling and prevents duplicate confirmation.
  // submitting 跟踪套件处理并防止重复确认。
  const [submitting, setSubmitting] = React.useState(false);
  // submittingRef synchronously prevents duplicate package handling before React rerenders.
  // submittingRef 在 React 重新渲染前同步阻止重复套件处理。
  const submittingRef = React.useRef(false);
  // error stores the latest local navigation or handling failure.
  // error 保存最近一次本地导航或处理失败。
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
    const currentKey = packagePathKey(listing.directory.path);
    let matchedPath: string | null = null;
    let matchedLength = -1;
    roots.forEach((root) => {
      const rootKey = packagePathKey(root.path);
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
      const nextListing = await onBrowse(path);
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
  }, [onBrowse]);

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

  // handleConfirm hands the selected package over before closing the picker.
  // handleConfirm 在关闭选择器前交出选中的套件。
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
      <DialogContent className="project-folder-picker-dialog package-file-picker-dialog" showCloseButton={!submitting}>
        <DialogHeader><DialogTitle>{props.title}</DialogTitle></DialogHeader>
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
                <button className={`project-folder-picker-row package-file${selectedPath === file.path ? " selected" : ""}`} disabled={loading || submitting} key={file.path} onClick={() => setSelectedPath(file.path)} type="button"><FileArchive size={16} /><span>{file.name}</span><small>{formatBytes(file.byteSize)}</small></button>
              ))}
              {listing && listing.directory.entries.length === 0 && listing.packages.length === 0 ? <div className="project-folder-picker-empty">{props.emptyLabel}</div> : null}
            </div>
          </section>
        </div>
        {error ? <div className="project-folder-picker-error">{error}</div> : null}
        <DialogFooter>
          <Button disabled={submitting} onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("conversation.action.cancel")}</Button>
          <Button disabled={!selectedPath || submitting || loading} onClick={() => void handleConfirm()} type="button">{submitting ? <Spinner /> : <Check size={16} />}{submitting ? props.busyLabel : props.confirmLabel}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
