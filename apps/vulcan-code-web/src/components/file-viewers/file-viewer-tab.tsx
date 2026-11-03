import { AppWindow, FileText } from "lucide-react";
import "@/ui-kit/styles/features/workspace-files.css";
import "@/ui-kit/styles/features/document-viewer.css";
import { useI18n } from "@/i18n";
import { PageEmptyState } from "@/components/page/page-feedback";
import { DocumentViewer } from "@/components/file-viewers/document-viewer";
import { fileViewerKind } from "@/components/file-viewers/file-viewer-kind";
import { useWorkspaceFileOpener } from "@/components/workspace/workspace-file-open-request";

// FileViewerTab is the right-panel viewer tab: it shows one file by absolute path, inside or outside the project,
// under a line naming the full path with a command that moves the file back into a window of its own.
// FileViewerTab 是右侧面板的查看页签：按绝对路径显示单个文件（项目内外均可），上方一行给出完整路径，并带有将文件移回
// 独立窗口的命令。
export default function FileViewerTab(props: { path: string; onUndock: () => void }) {
  const { t } = useI18n();
  const openFile = useWorkspaceFileOpener();
  const kind = fileViewerKind(props.path);
  const name = props.path.split(/[\\/]/).pop() ?? props.path;
  return (
    <div className="vk-docview-tab">
      <div className="vk-docview-path">
        <span title={props.path}>{props.path}</span>
        {openFile && kind ? (
          <button
            aria-label={t("fileWindow.undock")}
            className="vk-icon-btn"
            data-size="sm"
            onClick={() => {
              openFile(props.path);
              props.onUndock();
            }}
            title={t("fileWindow.undock")}
            type="button"
          >
            <AppWindow />
          </button>
        ) : null}
      </div>
      {kind ? (
        <div className="workspace-file-preview document">
          <DocumentViewer kind={kind} name={name} path={props.path} />
        </div>
      ) : (
        <PageEmptyState icon={<FileText />} size="panel" title={t("workspaceFilePreview.unsupportedTitle")} />
      )}
    </div>
  );
}
