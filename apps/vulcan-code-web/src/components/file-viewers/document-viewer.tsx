import React from "react";
import { localMediaSourceUrl } from "@/resources/localMedia";
import { MediaPathContextMenu } from "@/components/media/media-path-context-menu";
import { DocumentViewerLoading } from "@/components/file-viewers/document-viewer-states";
import type { FileViewerKind } from "@/components/file-viewers/file-viewer-kind";

// Each document viewer, its library and its stylesheet load only when a document of that kind opens.
// 每个文档查看器及其库和样式表仅在打开对应类型的文档时加载。
const LazyPdfViewer = React.lazy(() => import("@/components/file-viewers/pdf-document-viewer"));
const LazyDocxViewer = React.lazy(() => import("@/components/file-viewers/docx-document-viewer"));
const LazySheetViewer = React.lazy(() => import("@/components/file-viewers/sheet-document-viewer"));
const LazyOfficeViewer = React.lazy(() => import("@/components/file-viewers/office-document-viewer"));

// DocumentViewer shows the file at an absolute path in the viewer `kind` names. PDF, Word, image and video files are
// fetched through the local media route; spreadsheets are read by the service, and Office documents are converted to
// PDF by it.
// DocumentViewer 用 `kind` 指明的查看器显示绝对路径处的文件。PDF、Word、图片与视频文件经本地媒体路由获取；电子表格由
// 服务读取，Office 文档由服务转换为 PDF。
export function DocumentViewer(props: { path: string; name: string; kind: FileViewerKind }) {
  if (props.kind === "image" || props.kind === "video") {
    return (
      <div className="workspace-file-preview media">
        <MediaPathContextMenu locator={props.path}>
          {props.kind === "image"
            ? <img alt={props.name} decoding="async" src={localMediaSourceUrl(props.path)} />
            : <video aria-label={props.name} controls preload="metadata" src={localMediaSourceUrl(props.path)} />}
        </MediaPathContextMenu>
      </div>
    );
  }
  return (
    <React.Suspense fallback={<DocumentViewerLoading />}>
      {props.kind === "pdf" ? <LazyPdfViewer key={props.path} name={props.name} url={localMediaSourceUrl(props.path)} /> : null}
      {props.kind === "docx" ? <LazyDocxViewer key={props.path} name={props.name} url={localMediaSourceUrl(props.path)} /> : null}
      {props.kind === "sheet" ? <LazySheetViewer key={props.path} name={props.name} path={props.path} /> : null}
      {props.kind === "office" ? <LazyOfficeViewer key={props.path} name={props.name} path={props.path} /> : null}
    </React.Suspense>
  );
}
