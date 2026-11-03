// FileViewerKind names the read-only viewer that shows a file by itself, outside the project files panel too.
// FileViewerKind 指明单独显示文件的只读查看器，在项目文件面板之外同样适用。
export type FileViewerKind = "pdf" | "docx" | "sheet" | "image" | "video";

// FILE_VIEWER_EXTENSIONS maps the extensions the viewers read to their viewer. Documents match the service's
// "document" file kind; images and videos are the ones the local media route serves and browsers play.
// FILE_VIEWER_EXTENSIONS 将查看器可读取的扩展名映射到对应查看器。文档与服务的 "document" 文件类别一致；图片与视频是本地
// 媒体路由提供且浏览器可播放的类型。
const FILE_VIEWER_EXTENSIONS: Record<string, FileViewerKind> = {
  pdf: "pdf",
  docx: "docx",
  xlsx: "sheet",
  xlsm: "sheet",
  xlsb: "sheet",
  xls: "sheet",
  ods: "sheet",
  png: "image",
  jpg: "image",
  jpeg: "image",
  webp: "image",
  gif: "image",
  bmp: "image",
  svg: "image",
  mp4: "video",
  webm: "video",
  mov: "video",
  m4v: "video",
};

// fileViewerKind returns the viewer for a file path, or null when no viewer reads it.
// fileViewerKind 返回文件路径对应的查看器；没有查看器能读取时返回 null。
export function fileViewerKind(path: string): FileViewerKind | null {
  const extension = /\.([A-Za-z0-9]+)$/.exec(path)?.[1]?.toLowerCase() ?? "";
  return Object.hasOwn(FILE_VIEWER_EXTENSIONS, extension) ? FILE_VIEWER_EXTENSIONS[extension] : null;
}
