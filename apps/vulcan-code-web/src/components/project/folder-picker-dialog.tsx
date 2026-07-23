import React from "react";
import {
  ArrowUp,
  Check,
  ChevronRight,
  Folder,
  FolderOpen,
  HardDrive,
  RefreshCw,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useI18n } from "@/i18n";
import type { ProjectDirectoryListing } from "@/types";

// projectFolderPickerPathKey normalizes a path for picker root matching.
// projectFolderPickerPathKey 将路径归一化用于选择器根目录匹配。
function projectFolderPickerPathKey(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/g, "").toLocaleLowerCase() || "/";
}

// ProjectFolderPickerDialogProps groups service-backed project directory picker inputs.
// ProjectFolderPickerDialogProps 汇总服务驱动的项目目录选择器输入。
export interface ProjectFolderPickerDialogProps {
  // Optional dialog title override for host features reusing the directory browser.
  // 宿主功能复用目录浏览器时使用的可选弹窗标题覆盖。
  title?: string;
  // Optional confirmation label override.
  // 可选的确认按钮文案覆盖。
  confirmLabel?: string;
  // Optional native directory opened immediately when the dialog becomes visible.
  // 对话框变为可见时立即打开的可选原生目录。
  initialPath?: string | null;
  // Whether the picker dialog is currently open.
  // 当前选择器弹窗是否打开。
  open: boolean;
  // Handles dialog visibility changes.
  // 处理弹窗可见性变化。
  onOpenChange: (open: boolean) => void;
  // Loads one directory listing from the Core Service.
  // 从 Core Service 加载单个目录列表。
  onBrowse: (path: string | null) => Promise<ProjectDirectoryListing>;
  // Confirms the selected directory path.
  // 确认选中的目录路径。
  onConfirm: (path: string) => Promise<void>;
}

// ProjectFolderPickerDialog renders a service-host directory browser.
// ProjectFolderPickerDialog 渲染服务宿主目录浏览器。
export function ProjectFolderPickerDialog({
  title,
  confirmLabel,
  initialPath,
  open,
  onBrowse,
  onConfirm,
  onOpenChange,
}: ProjectFolderPickerDialogProps) {
  const { t } = useI18n();
  // listing stores the current directory listing returned by the service.
  // listing 保存服务返回的当前目录列表。
  const [listing, setListing] = React.useState<ProjectDirectoryListing | null>(null);
  // pathInput stores the editable path field.
  // pathInput 保存可编辑路径输入框。
  const [pathInput, setPathInput] = React.useState("");
  // loading tracks directory listing requests.
  // loading 跟踪目录列表请求。
  const [loading, setLoading] = React.useState(false);
  // submitting tracks the final project-add request.
  // submitting 跟踪最终添加项目请求。
  const [submitting, setSubmitting] = React.useState(false);
  // submittingRef synchronously prevents duplicate confirmation before React rerenders.
  // submittingRef 在 React 重新渲染前同步阻止重复确认。
  const submittingRef = React.useRef(false);
  // error stores the latest picker error.
  // error 保存最近一次选择器错误。
  const [error, setError] = React.useState<string | null>(null);
  // browseSequenceRef rejects stale directory listing responses.
  // browseSequenceRef 丢弃过期目录列表响应。
  const browseSequenceRef = React.useRef(0);
  // directoryRoots stores service roots with backward-compatible fallback for older responses.
  // directoryRoots 保存服务根目录，并兼容旧响应缺失 roots 的情况。
  const directoryRoots = React.useMemo(() => listing?.roots ?? [], [listing?.roots]);
  // activeRootPath stores the root currently owning the displayed directory.
  // activeRootPath 保存当前显示目录所属的根目录路径。
  const activeRootPath = React.useMemo(() => {
    if (!listing) {
      return null;
    }
    const currentKey = projectFolderPickerPathKey(listing.path);
    let matchedPath: string | null = null;
    let matchedLength = -1;
    for (const root of directoryRoots) {
      const rootKey = projectFolderPickerPathKey(root.path);
      const rootMatches = rootKey === "/" ? currentKey.startsWith("/") : currentKey === rootKey || currentKey.startsWith(`${rootKey}/`);
      if (rootMatches && rootKey.length > matchedLength) {
        matchedPath = root.path;
        matchedLength = rootKey.length;
      }
    }
    return matchedPath;
  }, [directoryRoots, listing]);

  // loadDirectory requests one directory listing and updates picker state.
  // loadDirectory 请求单个目录列表并更新选择器状态。
  const loadDirectory = React.useCallback(async (path: string | null) => {
    const sequence = browseSequenceRef.current + 1;
    browseSequenceRef.current = sequence;
    setLoading(true);
    setError(null);
    try {
      const nextListing = await onBrowse(path);
      if (browseSequenceRef.current !== sequence) {
        return;
      }
      setListing(nextListing);
      setPathInput(nextListing.path);
    } catch (nextError) {
      if (browseSequenceRef.current === sequence) {
        setError(nextError instanceof Error ? nextError.message : String(nextError));
      }
    } finally {
      if (browseSequenceRef.current === sequence) {
        setLoading(false);
      }
    }
  }, [onBrowse]);

  React.useEffect(() => {
    if (!open) {
      return;
    }
    void loadDirectory(initialPath ?? null);
  }, [initialPath, loadDirectory, open]);

  // handleConfirm submits the current path as the new project directory.
  // handleConfirm 将当前路径作为新项目目录提交。
  const handleConfirm = React.useCallback(async () => {
    const selectedPath = pathInput.trim() || listing?.path || "";
    if (!selectedPath || submittingRef.current) {
      return;
    }
    submittingRef.current = true;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm(selectedPath);
      onOpenChange(false);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  }, [listing?.path, onConfirm, onOpenChange, pathInput]);

  // handlePathSubmit loads the manually entered path.
  // handlePathSubmit 加载手动输入的路径。
  const handlePathSubmit = React.useCallback((event: React.FormEvent) => {
    event.preventDefault();
    void loadDirectory(pathInput.trim() || null);
  }, [loadDirectory, pathInput]);

  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !submittingRef.current && onOpenChange(nextOpen)}>
      <DialogContent
        className="project-folder-picker-dialog"
        onEscapeKeyDown={(event) => {
          if (submittingRef.current) {
            event.preventDefault();
          }
        }}
        onInteractOutside={(event) => {
          if (submittingRef.current) {
            event.preventDefault();
          }
        }}
        showCloseButton={!submitting}
      >
        <DialogHeader>
          <DialogTitle>{title ?? t("projectFolderPicker.title")}</DialogTitle>
        </DialogHeader>
        <form className="project-folder-picker-path-row" onSubmit={handlePathSubmit}>
          <Input
            aria-label={t("projectFolderPicker.pathAria")}
            disabled={loading || submitting}
            onChange={(event) => setPathInput(event.target.value)}
            value={pathInput}
          />
          <Button aria-label={t("projectFolderPicker.openPath")} disabled={loading || submitting} size="icon" type="submit" variant="outline">
            {loading ? <Spinner className="size-4" /> : <ChevronRight size={16} />}
          </Button>
        </form>
        <div className="project-folder-picker-toolbar">
          <Button
            disabled={!listing?.parentPath || loading || submitting}
            onClick={() => void loadDirectory(listing?.parentPath ?? null)}
            size="sm"
            type="button"
            variant="outline"
          >
            <ArrowUp size={15} />
            {t("projectFolderPicker.parent")}
          </Button>
          <Button
            disabled={!listing || loading || submitting}
            onClick={() => void loadDirectory(listing?.path ?? null)}
            size="sm"
            type="button"
            variant="outline"
          >
            <RefreshCw size={15} />
            {t("skills.action.refresh")}
          </Button>
        </div>
        <div className="project-folder-picker-browser">
          <aside aria-label={t("projectFolderPicker.rootsAria")} className="project-folder-picker-roots">
            {directoryRoots.map((root) => (
              <button
                className={root.path === activeRootPath ? "project-folder-picker-root active" : "project-folder-picker-root"}
                disabled={loading || submitting}
                key={root.path}
                onClick={() => void loadDirectory(root.path)}
                type="button"
              >
                <HardDrive size={16} />
                <span>{root.label}</span>
              </button>
            ))}
            {listing && directoryRoots.length === 0 ? (
              <div className="project-folder-picker-empty">{t("projectFolderPicker.noRoots")}</div>
            ) : null}
          </aside>
          <section className="project-folder-picker-directory">
            <div className="project-folder-picker-current">
              <FolderOpen size={16} />
              <span>{listing?.path ?? t("workspaceFiles.loading")}</span>
            </div>
            <div className="project-folder-picker-list" role="list">
              {listing?.entries.map((entry) => (
                <button
                  className="project-folder-picker-row"
                  disabled={loading || submitting}
                  key={entry.path}
                  onClick={() => void loadDirectory(entry.path)}
                  type="button"
                >
                  <Folder size={16} />
                  <span>{entry.name}</span>
                  {entry.hidden ? <small>{t("projectFolderPicker.hidden")}</small> : null}
                </button>
              ))}
              {listing && listing.entries.length === 0 ? (
                <div className="project-folder-picker-empty">{t("projectFolderPicker.noChildren")}</div>
              ) : null}
            </div>
          </section>
        </div>
        {error ? <div className="project-folder-picker-error">{error}</div> : null}
        {listing?.notice ? <div className="project-folder-picker-notice">{listing.notice}</div> : null}
        <DialogFooter>
          <Button disabled={submitting} onClick={() => onOpenChange(false)} type="button" variant="outline">
            {t("conversation.action.cancel")}
          </Button>
          <Button disabled={!listing?.canSelect || submitting || loading} onClick={handleConfirm} type="button">
            {submitting ? <Spinner className="size-4" /> : <Check size={16} />}
            {submitting ? t("projectFolderPicker.adding") : (confirmLabel ?? t("sidebar.addProject"))}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
