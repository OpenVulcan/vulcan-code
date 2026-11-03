// PDFJS_DATA_BASE_PATH is where the build copies pdf.js data folders (CMaps, standard fonts, wasm decoders and ICC
// profiles), relative to the app base; vite.config.ts serves and copies them there.
// PDFJS_DATA_BASE_PATH 是构建复制 pdf.js 数据文件夹（CMap、标准字体、wasm 解码器与 ICC 配置）的位置，相对于应用
// 基础路径；vite.config.ts 在该处提供并复制这些文件夹。
export const PDFJS_DATA_BASE_PATH = "pdfjs";

// PDFJS_DATA_FOLDERS lists the pdf.js package folders the viewer loads on demand.
// PDFJS_DATA_FOLDERS 列出查看器按需加载的 pdf.js 包文件夹。
export const PDFJS_DATA_FOLDERS = ["cmaps", "standard_fonts", "wasm", "iccs"] as const;
