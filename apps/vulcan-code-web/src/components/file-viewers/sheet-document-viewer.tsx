import React from "react";
import "@/ui-kit/styles/features/document-viewer.css";
import { useI18n } from "@/i18n";
import { PageEmptyState, PageNotice } from "@/components/page/page-feedback";
import { loadWorkspaceSheetPreview } from "@/service/workbench-snapshot-service";
import type { WorkspaceSheetPreview } from "@/types";
import { DocumentViewerFailure, DocumentViewerLoading } from "@/components/file-viewers/document-viewer-states";

// sheetColumnName returns the spreadsheet letter name of a zero-based column, such as 0 → "A" and 27 → "AB".
// sheetColumnName 返回从 0 开始的列对应的电子表格字母名称，例如 0 → "A"、27 → "AB"。
export function sheetColumnName(column: number): string {
  let name = "";
  for (let rest = column + 1; rest > 0; rest = Math.floor((rest - 1) / 26)) {
    name = String.fromCharCode(65 + ((rest - 1) % 26)) + name;
  }
  return name;
}

// SheetDocumentViewer shows the leading cells of a spreadsheet read-only, one sheet at a time, with sheet tabs,
// column letters and row numbers; the service caps how much of a sheet it reads.
// SheetDocumentViewer 只读显示电子表格的前部单元格，每次一个工作表，带工作表页签、列字母与行号；服务限制单个工作表的
// 读取量。
export default function SheetDocumentViewer(props: { path: string; name: string }) {
  const { t } = useI18n();
  const [sheetName, setSheetName] = React.useState<string | null>(null);
  const [preview, setPreview] = React.useState<WorkspaceSheetPreview | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let current = true;
    setError(null);
    setPreview(null);
    loadWorkspaceSheetPreview(props.path, sheetName).then(
      (result) => { if (current) setPreview(result); },
      (reason: unknown) => { if (current) setError(reason instanceof Error ? reason.message : String(reason)); },
    );
    return () => { current = false; };
  }, [props.path, sheetName]);

  const columns = preview ? Math.max(0, ...preview.rows.map((row) => row.length)) : 0;
  return (
    <div className="vk-docview" data-kind="sheet">
      {preview && preview.sheetNames.length > 1 ? (
        <div className="vk-docview-sheets" role="tablist" aria-label={props.name}>
          {preview.sheetNames.map((name) => (
            <button
              aria-selected={name === preview.sheetName}
              className="vk-docview-sheet"
              key={name}
              onClick={() => setSheetName(name)}
              role="tab"
              type="button"
            >
              {name}
            </button>
          ))}
        </div>
      ) : null}
      {preview?.truncated ? (
        <PageNotice tone="info">
          {t("documentViewer.sheetTruncated")
            .replace("{rows}", String(preview.rows.length))
            .replace("{columns}", String(columns))
            .replace("{totalRows}", String(preview.totalRows))
            .replace("{totalColumns}", String(preview.totalColumns))}
        </PageNotice>
      ) : null}
      <div className="vk-docview-body">
        {preview ? (
          <div className="vk-docview-scroll" data-find-scope="">
            {preview.rows.length === 0 ? (
              <PageEmptyState size="panel" title={t("documentViewer.sheetEmpty")} />
            ) : (
              <table className="vk-docview-grid">
                <thead>
                  <tr>
                    <th aria-hidden="true" />
                    {Array.from({ length: columns }, (_, index) => <th key={index} scope="col">{sheetColumnName(preview.firstColumn + index)}</th>)}
                  </tr>
                </thead>
                <tbody>
                  {preview.rows.map((row, rowIndex) => (
                    <tr key={rowIndex}>
                      <th scope="row">{preview.firstRow + rowIndex + 1}</th>
                      {Array.from({ length: columns }, (_, index) => <td key={index}>{row[index] ?? ""}</td>)}
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        ) : null}
        {!preview && !error ? <DocumentViewerLoading /> : null}
        {error ? <DocumentViewerFailure detail={error} /> : null}
      </div>
    </div>
  );
}
