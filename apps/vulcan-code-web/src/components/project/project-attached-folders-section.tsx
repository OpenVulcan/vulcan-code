import React from "react";
import { FolderTree, GitBranch, Plus, RefreshCw, X } from "lucide-react";
import { useI18n } from "@/i18n";
import type { WorkspaceGitRepositorySummary } from "@/diff/types";
import { browseProjectDirectories } from "@/service/project-actions-service";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import type { ProjectSummary } from "@/types";
import { ProjectFolderPickerDialog } from "./folder-picker-dialog";
import { detectProjectAttachedFolderGit, setProjectAttachedFolder, useProjectAttachedFolders } from "./project-attached-folders-state";

// Renders the registered folders and their Git detection results for the supplied project.
// 渲染指定项目已登记的目录及各目录的 Git 检测结果。
export function ProjectAttachedFoldersSection({ project }: { project: ProjectSummary }) {
  const { t } = useI18n();
  // Snapshot is scoped to the visible project and updated after an acknowledged write.
  // 快照限定在当前项目，写入成功后更新。
  const snapshot = useProjectAttachedFolders(project.id);
  // Picker is mounted only during an explicit add action.
  // 仅在用户显式添加目录时挂载选择器。
  const [pickerOpen, setPickerOpen] = React.useState(false);
  // Revision allows the user to rerun discovery for all displayed folders.
  // 版本号让用户可以重新检测所有显示的目录。
  const [revision, setRevision] = React.useState(0);
  return (
    <div className="project-settings-section">
      <header className="project-settings-heading">
        <FolderTree size={18} />
        <div><strong>{t("projectAttachedFolders.title")}</strong><span>{t("projectAttachedFolders.description")}</span></div>
      </header>
      <div className="project-attached-folders-actions">
        <Button onClick={() => setPickerOpen(true)} size="sm" variant="outline"><Plus />{t("projectAttachedFolders.add")}</Button>
        {snapshot?.paths?.length ? (
          <IconButton aria-label={t("projectAttachedFolders.refreshGit")} title={t("projectAttachedFolders.refreshGit")} onClick={() => setRevision((current) => current + 1)}>
            <RefreshCw />
          </IconButton>
        ) : null}
      </div>
      {snapshot?.error ? <div role="alert">{snapshot.error}</div> : !snapshot ? (
        <span role="status">{t("workspaceFiles.loading")}</span>
      ) : snapshot.paths?.length ? (
        <ul className="project-attached-folders-list">
          {snapshot.paths.map((path) => <AttachedFolderRow key={path} path={path} projectId={project.id} revision={revision} />)}
        </ul>
      ) : <span className="project-attached-folder-status">{t("projectAttachedFolders.empty")}</span>}
      {pickerOpen ? (
        <ProjectFolderPickerDialog
          open
          title={t("projectAttachedFolders.add")}
          confirmLabel={t("projectAttachedFolders.add")}
          initialPath={project.path}
          onBrowse={browseProjectDirectories}
          onConfirm={(path) => setProjectAttachedFolder(project.id, path, true)}
          onOpenChange={setPickerOpen}
        />
      ) : null}
    </div>
  );
}

// Shows one path registered to projectId and detects its Git repositories when revision changes.
// 展示 projectId 的一条已登记路径，并在 revision 变更时检测其 Git 仓库。
function AttachedFolderRow({ path, projectId, revision }: { path: string; projectId: string; revision: number }) {
  const { t } = useI18n();
  // Detection stores this row's latest repository list or error.
  // 检测状态保存本行最新的仓库列表或错误。
  const [detection, setDetection] = React.useState<{ repositories: WorkspaceGitRepositorySummary[]; error: string | null } | null>(null);
  // Removal failures remain on the affected row.
  // 移除失败时在受影响的行显示错误。
  const [removeError, setRemoveError] = React.useState<string | null>(null);
  // Busy disables the remove control while the database update is pending.
  // 等待数据库更新时禁用移除按钮。
  const [busy, setBusy] = React.useState(false);
  // Associates this row's remove control with its exact directory name.
  // 将本行移除控件与其确切目录名称关联。
  const pathLabelId = React.useId();
  React.useEffect(() => {
    // Guard discovery results after removal, a refresh, or a project switch.
    // 在移除、刷新或切换项目后拦截已过时的检测结果。
    let current = true;
    setDetection(null);
    void detectProjectAttachedFolderGit(projectId, path).then(
      ({ repositories }) => { if (current) setDetection({ repositories, error: null }); },
      (error: unknown) => { if (current) setDetection({ repositories: [], error: error instanceof Error ? error.message : String(error) }); },
    );
    return () => { current = false; };
  }, [path, projectId, revision]);
  // Removes only the saved association and exposes an error when the service rejects it.
  // 只移除已保存的关联，服务拒绝时展示错误。
  async function remove() {
    setBusy(true);
    setRemoveError(null);
    try {
      await setProjectAttachedFolder(projectId, path, false);
    } catch (error) {
      setRemoveError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <li className="project-settings-panel project-attached-folder">
      <div className="project-attached-folder-heading">
        <strong id={pathLabelId} title={path}>{path}</strong>
        <IconButton disabled={busy} aria-describedby={pathLabelId} aria-label={t("projectAttachedFolders.remove")} title={t("projectAttachedFolders.remove")} onClick={() => void remove()}><X /></IconButton>
      </div>
      {removeError ? <div role="alert">{removeError}</div> : null}
      {!detection ? <span className="project-attached-folder-status">{t("projectAttachedFolders.detectingGit")}</span> : detection.error ? (
        <span className="project-attached-folder-status" role="status">{detection.error}</span>
      ) : detection.repositories.length ? (
        <ul aria-label={t("projectAttachedFolders.repositories")} className="project-attached-folders-list">
          {detection.repositories.map((repository) => (
            <li key={repository.repositoryKey} title={repository.repositoryRoot}><GitBranch size={14} />{repository.relativePath === "." ? repository.name : repository.relativePath}</li>
          ))}
        </ul>
      ) : <span className="project-attached-folder-status">{t("projectAttachedFolders.noGit")}</span>}
    </li>
  );
}
