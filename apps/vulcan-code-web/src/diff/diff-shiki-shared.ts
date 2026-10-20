import type { ThemedToken } from "shiki/core";

import type { DiffFile } from "./types";

// DiffShikiTokenSpan is the serializable token unit passed from the worker to the renderer.
// DiffShikiTokenSpan 是从 Worker 传给 renderer 的可序列化 token 单元。
export type DiffShikiTokenSpan = readonly [
  content: string,
  style: string,
];

// DIFF_SHIKI_LIGHT_THEME stores the Shiki theme used by the light WebUI diff renderer.
// DIFF_SHIKI_LIGHT_THEME 保存亮色 WebUI diff 渲染器使用的 Shiki 主题。
export const DIFF_SHIKI_LIGHT_THEME = "github-light";

// DIFF_SHIKI_DARK_THEME stores the Shiki theme used by the dark WebUI diff renderer.
// DIFF_SHIKI_DARK_THEME 保存暗色 WebUI diff 渲染器使用的 Shiki 主题。
export const DIFF_SHIKI_DARK_THEME = "github-dark";

// DiffShikiTheme describes the WebUI diff Shiki theme identifiers.
// DiffShikiTheme 描述 WebUI diff 使用的 Shiki 主题标识。
export type DiffShikiTheme = typeof DIFF_SHIKI_LIGHT_THEME | typeof DIFF_SHIKI_DARK_THEME;

// currentDiffShikiTheme returns the Shiki theme matching the active WebUI color mode.
// currentDiffShikiTheme 返回与当前 WebUI 颜色模式匹配的 Shiki 主题。
export function currentDiffShikiTheme(): DiffShikiTheme {
  return document.documentElement.dataset.vkScheme === "dark" ? DIFF_SHIKI_DARK_THEME : DIFF_SHIKI_LIGHT_THEME;
}

// DIFF_SHIKI_LANGUAGE_IDS is the explicit WebUI review language allowlist.
// DIFF_SHIKI_LANGUAGE_IDS 是 WebUI 审查高亮显式支持的语言白名单。
const DIFF_SHIKI_LANGUAGE_IDS = new Set([
  "bash",
  "bat",
  "c",
  "clojure",
  "cmake",
  "cpp",
  "csharp",
  "css",
  "dart",
  "diff",
  "docker",
  "dotenv",
  "elixir",
  "fsharp",
  "go",
  "graphql",
  "haskell",
  "html",
  "ini",
  "java",
  "javascript",
  "json",
  "jsonc",
  "jsx",
  "kotlin",
  "less",
  "lua",
  "make",
  "markdown",
  "mdx",
  "objective-c",
  "perl",
  "php",
  "powershell",
  "python",
  "ruby",
  "rust",
  "sass",
  "scala",
  "scss",
  "sql",
  "svelte",
  "swift",
  "toml",
  "tsx",
  "typescript",
  "vue",
  "xml",
  "yaml",
]);

// DIFF_SHIKI_LANGUAGE_ALIASES maps file extensions and common ids to canonical loaders.
// DIFF_SHIKI_LANGUAGE_ALIASES 将文件扩展名与常见语言标识映射到规范加载器。
const DIFF_SHIKI_LANGUAGE_ALIASES: Record<string, string> = {
  cjs: "javascript",
  cmd: "bat",
  cs: "csharp",
  cts: "typescript",
  cxx: "cpp",
  dockerfile: "docker",
  fs: "fsharp",
  gql: "graphql",
  h: "c",
  hpp: "cpp",
  htm: "html",
  js: "javascript",
  jsonl: "json",
  kt: "kotlin",
  kts: "kotlin",
  m: "objective-c",
  md: "markdown",
  mjs: "javascript",
  mm: "objective-c",
  mts: "typescript",
  objc: "objective-c",
  objectivec: "objective-c",
  ps1: "powershell",
  psd1: "powershell",
  psm1: "powershell",
  py: "python",
  rb: "ruby",
  rs: "rust",
  sh: "bash",
  shell: "bash",
  shellscript: "bash",
  ts: "typescript",
  webmanifest: "json",
  yml: "yaml",
  zsh: "bash",
};

// DIFF_HIGHLIGHT_EXTENSION_MAP maps common file extensions to Shiki language ids.
// DIFF_HIGHLIGHT_EXTENSION_MAP 将常见文件扩展名映射到 Shiki 语言标识。
const DIFF_HIGHLIGHT_EXTENSION_MAP: Record<string, string> = {
  bash: "bash",
  bat: "bat",
  c: "c",
  cc: "cpp",
  clj: "clojure",
  cmake: "cmake",
  cmd: "bat",
  cpp: "cpp",
  cs: "csharp",
  css: "css",
  cts: "typescript",
  cxx: "cpp",
  dart: "dart",
  diff: "diff",
  dockerfile: "docker",
  ex: "elixir",
  exs: "elixir",
  fs: "fsharp",
  go: "go",
  graphql: "graphql",
  h: "c",
  hpp: "cpp",
  hs: "haskell",
  html: "html",
  htm: "html",
  ini: "ini",
  java: "java",
  js: "javascript",
  json: "json",
  jsonc: "jsonc",
  jsonl: "json",
  jsx: "jsx",
  kt: "kotlin",
  kts: "kotlin",
  less: "less",
  lua: "lua",
  m: "objective-c",
  md: "markdown",
  mdx: "mdx",
  mjs: "javascript",
  mm: "objective-c",
  mts: "typescript",
  php: "php",
  pl: "perl",
  ps1: "powershell",
  psd1: "powershell",
  psm1: "powershell",
  py: "python",
  rb: "ruby",
  rs: "rust",
  sass: "sass",
  scala: "scala",
  scss: "scss",
  sh: "bash",
  sql: "sql",
  svelte: "svelte",
  swift: "swift",
  toml: "toml",
  ts: "typescript",
  tsx: "tsx",
  vue: "vue",
  webmanifest: "json",
  xml: "xml",
  yaml: "yaml",
  yml: "yaml",
  zsh: "bash",
};

// DIFF_HIGHLIGHT_FILE_NAME_MAP maps extensionless special files to Shiki language ids.
// DIFF_HIGHLIGHT_FILE_NAME_MAP 将无扩展特殊文件名映射到 Shiki 语言标识。
const DIFF_HIGHLIGHT_FILE_NAME_MAP: Record<string, string> = {
  ".bashrc": "bash",
  ".env": "dotenv",
  ".gitconfig": "ini",
  ".gitignore": "ini",
  ".npmrc": "ini",
  "cargo.lock": "toml",
  "cmakelists.txt": "cmake",
  dockerfile: "docker",
  makefile: "make",
  "package-lock.json": "json",
  "package.json": "json",
  "pnpm-lock.yaml": "yaml",
  "tsconfig.json": "jsonc",
};

// diffFileLanguageCache stores inferred Shiki languages per stable parsed file object.
// diffFileLanguageCache 按稳定解析文件对象缓存推断出的 Shiki 语言。
const diffFileLanguageCache = new WeakMap<DiffFile, string | null>();

// tokenSpansHtmlCache stores rendered HTML for stable Shiki token span arrays.
// tokenSpansHtmlCache 按稳定 Shiki token span 数组缓存已渲染 HTML。
const tokenSpansHtmlCache = new WeakMap<readonly DiffShikiTokenSpan[], string>();

// diffLanguageForFile infers a Shiki language id from one diff file path.
// diffLanguageForFile 从单个 diff 文件路径推断 Shiki 语言标识。
export function diffLanguageForFile(file: DiffFile): string | null {
  const cached = diffFileLanguageCache.get(file);
  if (cached !== undefined) {
    return cached;
  }
  const language = shikiLanguageForPath(displayPath(file));
  diffFileLanguageCache.set(file, language);
  return language;
}

// shikiLanguageForPath infers a Shiki language id from a file path's name or extension.
// shikiLanguageForPath 从文件路径的文件名或扩展名推断 Shiki 语言标识。
export function shikiLanguageForPath(filePath: string): string | null {
  const path = filePath.replace(/\\/g, "/").toLowerCase();
  const slashIndex = path.lastIndexOf("/");
  const fileName = slashIndex >= 0 ? path.slice(slashIndex + 1) : path;
  const dotIndex = fileName.lastIndexOf(".");
  const extension = dotIndex >= 0 ? fileName.slice(dotIndex + 1) : "";
  const specialName = DIFF_HIGHLIGHT_FILE_NAME_MAP[fileName];
  if (specialName && shikiLanguageExists(specialName)) {
    return specialName;
  }
  const mappedExtension = DIFF_HIGHLIGHT_EXTENSION_MAP[extension];
  if (mappedExtension && shikiLanguageExists(mappedExtension)) {
    return mappedExtension;
  }
  return shikiLanguageExists(extension) ? extension : null;
}

// normalizeShikiLanguage resolves aliases to canonical allowlisted Shiki language ids.
// normalizeShikiLanguage 将别名解析为白名单中的规范 Shiki 语言标识。
export function normalizeShikiLanguage(language: string | null): string | null {
  if (!language) {
    return null;
  }
  const normalized = language.toLowerCase();
  const canonicalLanguage = DIFF_SHIKI_LANGUAGE_ALIASES[normalized] ?? normalized;
  return DIFF_SHIKI_LANGUAGE_IDS.has(canonicalLanguage) ? canonicalLanguage : null;
}

// escapeHtml safely renders diagnostic text through dangerouslySetInnerHTML.
// escapeHtml 为 dangerouslySetInnerHTML 安全渲染诊断文本。
export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

// tokensToSpans converts Shiki tokens into serializable token span data.
// tokensToSpans 将 Shiki token 转换为可序列化的 token span 数据。
export function tokensToSpans(tokens: ThemedToken[]): DiffShikiTokenSpan[] {
  const spans: DiffShikiTokenSpan[] = new Array(tokens.length);
  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index];
    spans[index] = [token.content, tokenStyle(token)];
  }
  return spans;
}

// tokenSpansToHtml converts serializable token spans into compact inline HTML.
// tokenSpansToHtml 将可序列化 token span 转换为紧凑行内 HTML。
export function tokenSpansToHtml(spans: readonly DiffShikiTokenSpan[]): string {
  const cached = tokenSpansHtmlCache.get(spans);
  if (cached != null) {
    return cached;
  }
  let html = "";
  for (const span of spans) {
    const content = escapeHtml(span[0]);
    html += span[1] ? `<span style="${span[1]}">${content}</span>` : content;
  }
  tokenSpansHtmlCache.set(spans, html);
  return html;
}

// shikiLanguageExists returns whether the local Shiki allowlist supports one language.
// shikiLanguageExists 返回本地 Shiki 白名单是否支持某个语言。
function shikiLanguageExists(language: string): boolean {
  return Boolean(normalizeShikiLanguage(language));
}

// tokenStyle converts Shiki token style fields into a safe inline CSS string.
// tokenStyle 将 Shiki token 样式字段转换为安全的行内 CSS 字符串。
function tokenStyle(token: ThemedToken): string {
  let style = "";
  if (token.color) {
    style += `color:${sanitizeCssColor(token.color)}`;
  }
  if (token.bgColor) {
    style += `${style ? ";" : ""}background-color:${sanitizeCssColor(token.bgColor)}`;
  }
  if (token.fontStyle != null && token.fontStyle > 0) {
    if ((token.fontStyle & 1) !== 0) {
      style += `${style ? ";" : ""}font-style:italic`;
    }
    if ((token.fontStyle & 2) !== 0) {
      style += `${style ? ";" : ""}font-weight:700`;
    }
    if ((token.fontStyle & 4) !== 0) {
      style += `${style ? ";" : ""}text-decoration:underline`;
    }
  }
  return style;
}

// sanitizeCssColor keeps only hexadecimal theme colors in inline styles.
// sanitizeCssColor 仅允许十六进制主题颜色进入行内样式。
function sanitizeCssColor(color: string): string {
  return /^#[\da-f]{6,8}$/i.test(color) ? color : "inherit";
}

// displayPath returns the primary file path shown in the file header.
// displayPath 返回文件头中展示的主路径。
function displayPath(file: DiffFile): string {
  return file.newPath && file.newPath !== "/dev/null" ? file.newPath : file.oldPath;
}
