import { AppWindow } from "lucide-react";
import "@/ui-kit/styles/features/workspace-files.css";
import "@/ui-kit/styles/features/document-viewer.css";
import { useI18n } from "@/i18n";
import { DocumentViewer } from "@/components/file-viewers/document-viewer";
import { fileViewerKind } from "@/components/file-viewers/file-viewer-kind";
import { OutsideTextViewer } from "@/components/file-viewers/outside-text-viewer";
import { useWorkspaceFileOpener } from "@/components/workspace/workspace-file-open-request";

// FileViewerTab is the right-panel viewer tab: it shows one file by absolute path, inside or outside the project (a
// file no viewer reads as text), under a line naming the full path with a command that moves the file back into a
// window of its own.
// FileViewerTab 是右侧面板的查看页签：按绝对路径显示单个文件（项目内外均可，没有查看器读取的文件按文本显示），上方一行
// 给出完整路径，并带有将文件移回独立窗口的命令。
export default function FileViewerTab(props: { path: string; onUndock: () => void }) {
  const { t } = useI18n();
  const openFile = useWorkspaceFileOpener();
  const kind = fileViewerKind(props.path);
  const name = props.path.split(/[\\/]/).pop() ?? props.path;
  return (
    <div className="vk-docview-tab">
      <div className="vk-docview-path">
        <span title={props.path}>{props.path}</span>
        {openFile ? (
          <button
            aria-label={t("fileWindow.undock")}
            className="vk-icon-btn"
            data-size="sm"
            onClick={() => {
              openFile(props.path, null, "window");
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
        <OutsideTextViewer path={props.path} />
      )}
    </div>
  );
}
