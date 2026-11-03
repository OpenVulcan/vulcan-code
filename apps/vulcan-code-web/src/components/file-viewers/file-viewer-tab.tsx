import { FileText } from "lucide-react";
import "@/ui-kit/styles/features/workspace-files.css";
import "@/ui-kit/styles/features/document-viewer.css";
import { useI18n } from "@/i18n";
import { PageEmptyState } from "@/components/page/page-feedback";
import { DocumentViewer } from "@/components/file-viewers/document-viewer";
import { fileViewerKind } from "@/components/file-viewers/file-viewer-kind";

// FileViewerTab is the right-panel viewer tab: it shows one file by absolute path, inside or outside the project,
// under a line naming the full path.
// FileViewerTab 是右侧面板的查看页签：按绝对路径显示单个文件（项目内外均可），上方一行给出完整路径。
export default function FileViewerTab(props: { path: string }) {
  const { t } = useI18n();
  const kind = fileViewerKind(props.path);
  const name = props.path.split(/[\\/]/).pop() ?? props.path;
  return (
    <div className="vk-docview-tab">
      <div className="vk-docview-path" title={props.path}>{props.path}</div>
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
