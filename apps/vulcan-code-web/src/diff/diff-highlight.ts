import * as React from "react";
import hljs from "highlight.js/lib/core";
import type { LanguageFn } from "highlight.js";

import type { DiffFile } from "./types";

// HighlightLanguageModule describes one dynamically imported highlight.js language module.
// HighlightLanguageModule 描述一个动态导入的 highlight.js 语言模块。
interface HighlightLanguageModule {
  // Default export may contain the language factory in ESM builds.
  // 默认导出在 ESM 构建中可能包含语言工厂。
  default?: LanguageFn;
  // Some dev transforms expose the language factory under a named export.
  // 部分开发态转换会把语言工厂暴露在命名导出中。
  [key: string]: unknown;
}

// ImportMetaWithGlob exposes Vite's glob import in projects without vite/client ambient types.
// ImportMetaWithGlob 在未引入 vite/client 全局类型的项目中暴露 Vite 的 glob 导入。
type ImportMetaWithGlob = ImportMeta & {
  // glob builds a lazy loader map for matching modules.
  // glob 为匹配模块构建懒加载器映射。
  glob: <T>(pattern: string) => Record<string, () => Promise<T>>;
};

// DIFF_HIGHLIGHT_ROW_LIMIT disables syntax highlighting for large files before virtualization.
// DIFF_HIGHLIGHT_ROW_LIMIT 在虚拟化前对大文件关闭语法高亮。
export const DIFF_HIGHLIGHT_ROW_LIMIT = 2000;

// highlightLanguageModules contains every highlight.js language chunk and loads each only when needed.
// highlightLanguageModules 包含所有 highlight.js 语言分块，并且仅在需要时加载对应语言。
const highlightLanguageModules = (import.meta as ImportMetaWithGlob).glob<HighlightLanguageModule>("/node_modules/highlight.js/es/languages/*.js");

// pendingHighlightLanguageLoads de-duplicates concurrent language imports.
// pendingHighlightLanguageLoads 去重并发的语言导入请求。
const pendingHighlightLanguageLoads = new Map<string, Promise<void>>();

// DIFF_HIGHLIGHT_EXTENSION_MAP maps common file extensions to highlight.js language ids.
// DIFF_HIGHLIGHT_EXTENSION_MAP 将常见文件扩展名映射到 highlight.js 语言标识。
const DIFF_HIGHLIGHT_EXTENSION_MAP: Record<string, string> = {
  bash: "bash",
  bat: "dos",
  c: "c",
  cc: "cpp",
  clj: "clojure",
  cmake: "cmake",
  cmd: "dos",
  cpp: "cpp",
  cs: "csharp",
  css: "css",
  cts: "typescript",
  cxx: "cpp",
  dart: "dart",
  diff: "diff",
  dockerfile: "dockerfile",
  ex: "elixir",
  exs: "elixir",
  fs: "fsharp",
  go: "go",
  graphql: "graphql",
  h: "c",
  hpp: "cpp",
  hs: "haskell",
  html: "xml",
  htm: "xml",
  ini: "ini",
  java: "java",
  js: "javascript",
  json: "json",
  jsonc: "json",
  jsonl: "json",
  jsx: "javascript",
  kt: "kotlin",
  kts: "kotlin",
  less: "less",
  lua: "lua",
  m: "objectivec",
  md: "markdown",
  mdx: "markdown",
  mjs: "javascript",
  mm: "objectivec",
  mts: "typescript",
  php: "php",
  pl: "perl",
  ps1: "powershell",
  psd1: "powershell",
  psm1: "powershell",
  py: "python",
  rb: "ruby",
  rs: "rust",
  sass: "scss",
  scala: "scala",
  scss: "scss",
  sh: "bash",
  sql: "sql",
  svelte: "xml",
  swift: "swift",
  toml: "ini",
  ts: "typescript",
  tsx: "typescript",
  vue: "xml",
  webmanifest: "json",
  xml: "xml",
  yaml: "yaml",
  yml: "yaml",
  zsh: "bash",
};

// DIFF_HIGHLIGHT_FILE_NAME_MAP maps extensionless special files to highlight.js language ids.
// DIFF_HIGHLIGHT_FILE_NAME_MAP 将无扩展特殊文件名映射到 highlight.js 语言标识。
const DIFF_HIGHLIGHT_FILE_NAME_MAP: Record<string, string> = {
  ".bashrc": "bash",
  ".env": "bash",
  ".gitconfig": "ini",
  ".gitignore": "bash",
  ".npmrc": "ini",
  "cargo.lock": "ini",
  "cmakelists.txt": "cmake",
  "dockerfile": "dockerfile",
  "makefile": "makefile",
  "package-lock.json": "json",
  "package.json": "json",
  "pnpm-lock.yaml": "yaml",
  "tsconfig.json": "json",
};

// diffLanguageForFile infers a highlight.js language id from one diff file path.
// diffLanguageForFile 从单个 diff 文件路径推断 highlight.js 语言标识。
export function diffLanguageForFile(file: DiffFile): string | null {
  const path = displayPath(file).replace(/\\/g, "/").toLowerCase();
  const fileName = path.split("/").at(-1) ?? path;
  const extension = fileName.includes(".") ? fileName.split(".").at(-1) ?? "" : "";
  const specialName = DIFF_HIGHLIGHT_FILE_NAME_MAP[fileName];
  if (specialName) {
    return specialName;
  }
  const mappedExtension = DIFF_HIGHLIGHT_EXTENSION_MAP[extension];
  if (mappedExtension) {
    return mappedExtension;
  }
  return languageModuleExists(extension) ? extension : null;
}

// useHighlightLanguage registers one highlight.js language module on demand.
// useHighlightLanguage 按需注册单个 highlight.js 语言模块。
export function useHighlightLanguage(language: string | null): boolean {
  const [, refresh] = React.useReducer((value: number) => value + 1, 0);
  React.useEffect(() => {
    if (!language || hljs.getLanguage(language)) {
      return;
    }
    let cancelled = false;
    void ensureHighlightLanguage(language).then(() => {
      if (!cancelled) {
        refresh();
      }
    });
    return () => {
      cancelled = true;
    };
  }, [language]);
  return !language || Boolean(hljs.getLanguage(language));
}

// highlightedDiffLineHtml returns syntax-highlighted, HTML-escaped code for one diff line.
// highlightedDiffLineHtml 返回单条 diff 行经过语法高亮和 HTML 转义后的代码。
export function highlightedDiffLineHtml(line: string, language: string): string {
  try {
    return hljs.highlight(line, { language, ignoreIllegals: true }).value;
  } catch {
    return escapeHtml(line);
  }
}

// escapeHtml safely renders fallback text through dangerouslySetInnerHTML.
// escapeHtml 为 dangerouslySetInnerHTML 安全渲染兜底文本。
export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

// ensureHighlightLanguage imports and registers one language module if highlight.js supports it.
// ensureHighlightLanguage 在 highlight.js 支持时导入并注册单个语言模块。
async function ensureHighlightLanguage(language: string): Promise<void> {
  if (hljs.getLanguage(language)) {
    return;
  }
  const existingLoad = pendingHighlightLanguageLoads.get(language);
  if (existingLoad) {
    return existingLoad;
  }
  const modulePath = highlightLanguageModulePath(language);
  const loader = modulePath ? highlightLanguageModules[modulePath] : undefined;
  if (!loader) {
    return;
  }
  const load = loader()
    .then((module) => {
      const languageFactory = highlightLanguageFactory(module);
      if (languageFactory && !hljs.getLanguage(language)) {
        hljs.registerLanguage(language, languageFactory);
      }
    })
    .finally(() => pendingHighlightLanguageLoads.delete(language));
  pendingHighlightLanguageLoads.set(language, load);
  return load;
}

// highlightLanguageFactory extracts a highlight.js language factory from a lazy module.
// highlightLanguageFactory 从懒加载模块中提取 highlight.js 语言工厂。
function highlightLanguageFactory(module: HighlightLanguageModule): LanguageFn | null {
  if (typeof module.default === "function") {
    return module.default;
  }
  const namedFactory = Object.values(module).find((value) => typeof value === "function");
  return typeof namedFactory === "function" ? (namedFactory as LanguageFn) : null;
}

// languageModuleExists returns whether highlight.js has a bundled module for one language id.
// languageModuleExists 返回 highlight.js 是否内置某个语言标识对应的模块。
function languageModuleExists(language: string): boolean {
  return Boolean(highlightLanguageModulePath(language));
}

// highlightLanguageModulePath resolves the Vite glob key for one highlight.js language id.
// highlightLanguageModulePath 解析单个 highlight.js 语言标识对应的 Vite glob 键。
function highlightLanguageModulePath(language: string): string | null {
  const path = `/node_modules/highlight.js/es/languages/${language}.js`;
  return Object.prototype.hasOwnProperty.call(highlightLanguageModules, path) ? path : null;
}

// displayPath returns the primary file path shown in the file header.
// displayPath 返回文件头中展示的主路径。
function displayPath(file: DiffFile): string {
  return file.newPath && file.newPath !== "/dev/null" ? file.newPath : file.oldPath;
}
