import React from "react";
import { FileWarning } from "lucide-react";
import "@/ui-kit/styles/features/document-viewer.css";
import { useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import { PageEmptyState } from "@/components/page/page-feedback";
import { localMediaSourceUrl } from "@/resources/localMedia";
import { loadWorkspaceOfficePdf } from "@/service/workspace-office-pdf-service";
import { localFileActionsAvailable, requestWorkspaceFileAction } from "@/components/workspace/workspace-file-actions";
import { DocumentViewerFailure, DocumentViewerLoading } from "@/components/file-viewers/document-viewer-states";
import PdfDocumentViewer from "@/components/file-viewers/pdf-document-viewer";
import type { WorkspaceOfficePdf } from "@/types";

// OfficeDocumentViewer shows a Word (.doc) or PowerPoint (.ppt/.pptx) file read-only: the service converts it to PDF
// with the host's LibreOffice, and the PDF viewer shows the result. Without LibreOffice it says so and, on the
// desktop, offers the system's default program.
// OfficeDocumentViewer 只读显示 Word（.doc）或 PowerPoint（.ppt/.pptx）文件：服务用宿主的 LibreOffice 将其转换为 PDF，
// 再由 PDF 查看器显示结果。没有 LibreOffice 时说明这一点，并在桌面端提供系统默认程序。
export default function OfficeDocumentViewer(props: { path: string; name: string }) {
  const { t } = useI18n();
  const [result, setResult] = React.useState<WorkspaceOfficePdf | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let current = true;
    loadWorkspaceOfficePdf(props.path).then(
      (value) => { if (current) setResult(value); },
      (reason: unknown) => { if (current) setError(reason instanceof Error ? reason.message : String(reason)); },
    );
    return () => { current = false; };
  }, [props.path]);

  if (result?.status === "converted") return <PdfDocumentViewer name={props.name} url={localMediaSourceUrl(result.pdfPath)} />;
  return (
    <div className="vk-docview" data-kind="office">
      <div className="vk-docview-body">
        {result?.status === "converterMissing" ? (
          <PageEmptyState
            action={localFileActionsAvailable() ? (
              <Button onClick={() => requestWorkspaceFileAction(props.path, "open")} type="button" variant="outline">
                {t("fileActions.openWithDefaultApp")}
              </Button>
            ) : null}
            description={t("documentViewer.converterMissingHint")}
            icon={<FileWarning />}
            size="panel"
            title={t("documentViewer.converterMissingTitle")}
          />
        ) : null}
        {!result && !error ? <DocumentViewerLoading label={t("documentViewer.converting")} /> : null}
        {error ? <DocumentViewerFailure detail={error} /> : null}
      </div>
    </div>
  );
}
