// FileViewerKind names the read-only viewer that shows a file by itself, outside the project files panel too.
// "office" documents are converted to PDF by the service's LibreOffice before the PDF viewer shows them.
// FileViewerKind 指明单独显示文件的只读查看器，在项目文件面板之外同样适用。"office" 文档先由服务的 LibreOffice 转换为
// PDF，再由 PDF 查看器显示。
export type FileViewerKind = "pdf" | "docx" | "sheet" | "office" | "image" | "video";

// FILE_VIEWER_EXTENSIONS lists, per viewer, the space-separated extensions it reads. Documents match the service's
// "document" file kind; images and videos are the ones the local media route serves and browsers play.
// FILE_VIEWER_EXTENSIONS 按查看器列出其可读取的扩展名（以空格分隔）。文档与服务的 "document" 文件类别一致；图片与视频
// 是本地媒体路由提供且浏览器可播放的类型。
const FILE_VIEWER_EXTENSIONS: Record<FileViewerKind, string> = {
  pdf: "pdf",
  docx: "docx",
  sheet: "xlsx xlsm xlsb xls ods",
  office: "doc ppt pptx",
  image: "png jpg jpeg webp gif bmp svg",
  video: "mp4 webm mov m4v",
};

// fileViewerKind returns the viewer for a file path, or null when no viewer reads it.
// fileViewerKind 返回文件路径对应的查看器；没有查看器能读取时返回 null。
export function fileViewerKind(path: string): FileViewerKind | null {
  const extension = /\.([A-Za-z0-9]+)$/.exec(path)?.[1]?.toLowerCase();
  const entry = Object.entries(FILE_VIEWER_EXTENSIONS).find(([, extensions]) => extensions.split(" ").includes(extension ?? ""));
  return entry ? entry[0] as FileViewerKind : null;
}
