import React from "react";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import { localMediaSourceUrl } from "@/resources/localMedia";

// markdownPlugins stores the Markdown extension set shared by inline and modal renderers.
// markdownPlugins 保存内联与弹窗渲染器共享的 Markdown 扩展集合。
const markdownPlugins = [remarkGfm];

// markdownSanitizers stores the HTML sanitization pipeline for untrusted transcript content.
// markdownSanitizers 保存用于不可信转录内容的 HTML 清理管线。
const markdownSanitizers = [rehypeSanitize];

// MarkdownImageWorkspacePathContext carries the current project root for relative image links.
// MarkdownImageWorkspacePathContext 为相对图片链接携带当前项目根目录。
const MarkdownImageWorkspacePathContext = React.createContext<string | null>(null);

// MarkdownImageWorkspacePathProvider provides the project path used by nested Markdown images.
// MarkdownImageWorkspacePathProvider 提供嵌套 Markdown 图片解析所需的项目路径。
export function MarkdownImageWorkspacePathProvider(props: {
  // Nested transcript or modal content that renders Markdown.
  // 会渲染 Markdown 的嵌套转录或弹窗内容。
  children: React.ReactNode;
  // Current project workspace path; blank values are treated as absent.
  // 当前项目工作区路径；空白值会按不存在处理。
  value?: string | null;
}) {
  return (
    <MarkdownImageWorkspacePathContext.Provider value={props.value?.trim() || null}>
      {props.children}
    </MarkdownImageWorkspacePathContext.Provider>
  );
}

// MarkdownMessage renders assistant Markdown with Vulcan-specific component styling.
// MarkdownMessage 使用 Vulcan 定制组件样式渲染 assistant Markdown。
export const MarkdownMessage = React.memo(function MarkdownMessage(props: {
  // Markdown source content to render.
  // 需要渲染的 Markdown 源内容。
  content: string;
  // Whether to bypass viewport lazy mounting for overlays and modal surfaces.
  // 是否为浮层和弹窗界面绕过视口懒挂载。
  eager?: boolean;
  // Whether the content is still growing and should avoid parsing incomplete block syntax.
  // 内容是否仍在增长，需要避免解析未闭合的块级语法。
  streaming?: boolean;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  const workspacePath = React.useContext(MarkdownImageWorkspacePathContext);
  // finalizedContent keeps non-streaming Markdown normalized only when the stable body changes.
  // finalizedContent 让非流式 Markdown 只在稳定正文变化时执行归一化。
  const finalizedContent = React.useMemo(
    () => props.streaming ? props.content : normalizeMarkdownLocalImageLinks(props.content, workspacePath),
    [props.content, props.streaming, workspacePath],
  );
  return (
    <div className="markdown-message">
      {props.streaming ? (
        <StreamingMarkdownMessage
          content={props.content}
          onOpenMedia={props.onOpenMedia}
          workspacePath={workspacePath}
        />
      ) : (
        <LazyMarkdownMessage
          content={finalizedContent}
          eager={props.eager}
          onOpenMedia={props.onOpenMedia}
          workspacePath={workspacePath}
        />
      )}
    </div>
  );
});

// StreamingMarkdownMessage renders the active Markdown block as one complete region.
// StreamingMarkdownMessage 将活跃 Markdown 块作为完整区域渲染。
function StreamingMarkdownMessage(props: {
  // Raw Markdown content for the active streaming block.
  // 活跃流式块的原始 Markdown 内容。
  content: string;
  // Current project workspace path for image source resolution.
  // 当前项目工作区路径，用于解析图片来源。
  workspacePath: string | null;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  const normalizedContent = React.useMemo(
    () => normalizeMarkdownLocalImageLinks(props.content, props.workspacePath),
    [props.content, props.workspacePath],
  );
  return (
    <MarkdownContentRenderer
      content={normalizedContent}
      onOpenMedia={props.onOpenMedia}
      workspacePath={props.workspacePath}
    />
  );
}

// MarkdownContentRenderer renders one already-normalized Markdown fragment.
// MarkdownContentRenderer 渲染单个已经规范化的 Markdown 片段。
function MarkdownContentRenderer(props: {
  // Markdown fragment to render.
  // 需要渲染的 Markdown 片段。
  content: string;
  // Current project workspace path for image source resolution.
  // 当前项目工作区路径，用于解析图片来源。
  workspacePath: string | null;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  const components = React.useMemo(() => ({
    a: ({ children, href }: { children?: React.ReactNode; href?: string }) => (
      <a href={href} rel="noreferrer" target="_blank">{children}</a>
    ),
    code: ({ children, className }: { children?: React.ReactNode; className?: string }) => {
      const inline = !className;
      return inline ? <code className="inline-code">{children}</code> : <code className={className}>{children}</code>;
    },
    img: ({ alt, src, title }: { alt?: string; src?: string; title?: string }) => {
      const width = markdownImageWidthFromTitle(title);
      const imageSource = normalizeMarkdownImageSource(src, props.workspacePath) ?? src;
      const imageAttachment = markdownImageAttachment(imageSource, alt);
      const imageStyle = width ? { width, maxWidth: 800 } : undefined;
      if (imageAttachment && props.onOpenMedia) {
        return (
          <button
            className="markdown-image-button"
            onClick={() => props.onOpenMedia?.(imageAttachment)}
            style={imageStyle}
            type="button"
          >
            <img
              alt={alt ?? ""}
              decoding="async"
              loading="lazy"
              src={imageSource}
            />
          </button>
        );
      }
      return (
        <img
          alt={alt ?? ""}
          decoding="async"
          loading="lazy"
          src={imageSource}
          style={imageStyle}
        />
      );
    },
    pre: ({ children }: { children?: React.ReactNode }) => <pre className="markdown-code-block">{children}</pre>,
    table: ({ children }: { children?: React.ReactNode }) => <div className="markdown-table-wrap"><table>{children}</table></div>,
  }), [props.onOpenMedia, props.workspacePath]);
  return (
    <ReactMarkdown
      components={components}
      rehypePlugins={markdownSanitizers}
      remarkPlugins={markdownPlugins}
    >
      {props.content}
    </ReactMarkdown>
  );
}

// LazyMarkdownMessage keeps one complete Markdown document mounted for stable layout.
// LazyMarkdownMessage 保持完整 Markdown 文档挂载以获得稳定布局。
function LazyMarkdownMessage(props: {
  // Already-normalized Markdown content.
  // 已经规范化的 Markdown 内容。
  content: string;
  // Kept for call-site compatibility after removing chunk-level unmounting.
  // 移除分块卸载后保留该参数以兼容现有调用点。
  eager?: boolean;
  // Current project workspace path for image source resolution.
  // 当前项目工作区路径，用于解析图片来源。
  workspacePath: string | null;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  return (
    <MarkdownContentRenderer
      content={props.content}
      onOpenMedia={props.onOpenMedia}
      workspacePath={props.workspacePath}
    />
  );
}

// mediaSourceUrl converts local media paths into URLs usable by either Tauri or browser preview mode.
// mediaSourceUrl 将本地媒体路径转换为 Tauri 或浏览器预览模式可用的 URL。
function mediaSourceUrl(path: string): string {
  return localMediaSourceUrl(path);
}

// normalizeMarkdownImageSource converts generated local image paths into renderable media URLs.
// normalizeMarkdownImageSource 将 Markdown 中的生成本地图片路径转换为可渲染媒体 URL。
function normalizeMarkdownImageSource(src: string | undefined, workspacePath?: string | null): string | undefined {
  const value = src?.trim();
  if (!value) {
    return src;
  }
  const unwrapped = value.startsWith("<") && value.endsWith(">") ? value.slice(1, -1).trim() : value;
  if (/^vulcan-media:/i.test(unwrapped) || /^https?:\/\/vulcan-media\.localhost(?:\/|$)/i.test(unwrapped)) {
    return mediaSourceUrl(unwrapped);
  }
  if (/^(https?:|data:|blob:|asset:)/i.test(unwrapped)) {
    return unwrapped;
  }
  if (/^file:/i.test(unwrapped)) {
    return mediaSourceUrl(unwrapped);
  }
  if (looksLikeLocalFilesystemPath(unwrapped)) {
    return mediaSourceUrl(unwrapped);
  }
  const relativeSource = resolveWorkspaceRelativeMarkdownImageSource(unwrapped, workspacePath);
  if (relativeSource) {
    return relativeSource;
  }
  return src;
}

// markdownImageAttachment converts one rendered Markdown image URL into a preview attachment.
// markdownImageAttachment 将单个已渲染 Markdown 图片 URL 转换为预览附件。
function markdownImageAttachment(src: string | undefined, alt: string | undefined): GeneratedMediaAttachment | null {
  const value = src?.trim();
  if (!value) {
    return null;
  }
  const cleanValue = value.startsWith("<") && value.endsWith(">") ? value.slice(1, -1).trim() : value;
  const fileName = markdownImageFileName(cleanValue, alt);
  return {
    kind: "image",
    fileName,
    localPath: cleanValue,
    mimeType: markdownImageMimeType(cleanValue),
  };
}

// markdownImageFileName returns a stable display name for Markdown image previews.
// markdownImageFileName 为 Markdown 图片预览返回稳定展示名称。
function markdownImageFileName(src: string, alt: string | undefined): string {
  const cleanAlt = alt?.replace(/\|\s*\d+\s*$/u, "").trim();
  if (cleanAlt) {
    return cleanAlt;
  }
  const withoutQuery = src.split(/[?#]/u, 1)[0] ?? src;
  const normalized = withoutQuery.replace(/\\/g, "/");
  const tail = normalized.split("/").filter(Boolean).pop();
  return tail ? decodeURIComponentSafe(tail) : "markdown-image";
}

// markdownImageMimeType infers a lightweight MIME type from a Markdown image URL.
// markdownImageMimeType 从 Markdown 图片 URL 推断轻量 MIME 类型。
function markdownImageMimeType(src: string): string | undefined {
  if (/^data:image\/([^;,]+)/i.test(src)) {
    const match = src.match(/^data:(image\/[^;,]+)/i);
    return match?.[1];
  }
  const clean = src.split(/[?#]/u, 1)[0]?.toLowerCase() ?? "";
  if (clean.endsWith(".png")) return "image/png";
  if (clean.endsWith(".jpg") || clean.endsWith(".jpeg")) return "image/jpeg";
  if (clean.endsWith(".webp")) return "image/webp";
  if (clean.endsWith(".gif")) return "image/gif";
  if (clean.endsWith(".svg")) return "image/svg+xml";
  return undefined;
}

// decodeURIComponentSafe decodes URL text while tolerating malformed percent escapes.
// decodeURIComponentSafe 在容忍非法百分号转义的情况下解码 URL 文本。
function decodeURIComponentSafe(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

// normalizeMarkdownLocalImageLinks rewrites local image links before Markdown sanitization removes them.
// normalizeMarkdownLocalImageLinks 在 Markdown 清理移除本地路径前重写本地图片链接。
function normalizeMarkdownLocalImageLinks(content: string, workspacePath?: string | null): string {
  return content.replace(/!\[([^\]\n]*)\]\(([^)\n]+)\)/g, (full, alt: string, destination: string, offset: number) => {
    const normalized = normalizeMarkdownImageDestination(destination, workspacePath);
    const width = markdownImageWidthFromAlt(alt);
    const cleanAlt = width ? alt.replace(/\|\s*\d+\s*$/u, "").trim() : alt;
    const nextDestination = normalized ?? destination.trim();
    const widthTitle = width ? ` "width=${width}"` : "";
    const imageMarkdown = normalized || width ? `![${cleanAlt}](${nextDestination}${widthTitle})` : full;
    return `${markdownImageLeadingBlockPrefix(content, offset)}${imageMarkdown}`;
  });
}

// markdownImageLeadingBlockPrefix returns the newline prefix needed to render an image as its own block.
// markdownImageLeadingBlockPrefix 返回将图片渲染为独立块所需的换行前缀。
function markdownImageLeadingBlockPrefix(content: string, offset: number): string {
  const previousContent = content.slice(0, Math.max(0, offset));
  if (previousContent.trim().length === 0) {
    return "";
  }
  if (/(?:\r?\n)[ \t]*(?:\r?\n)[ \t]*$/u.test(previousContent)) {
    return "";
  }
  const lineStart = Math.max(content.lastIndexOf("\n", offset - 1), content.lastIndexOf("\r", offset - 1)) + 1;
  return content.slice(lineStart, offset).trim().length > 0 ? "\n\n" : "\n";
}

// normalizeMarkdownImageDestination extracts and rewrites one Markdown image destination.
// normalizeMarkdownImageDestination 提取并重写单个 Markdown 图片目标。
function normalizeMarkdownImageDestination(destination: string, workspacePath?: string | null): string | null {
  const trimmed = destination.trim();
  if (!trimmed) {
    return null;
  }
  const sizedDestination = markdownImageSizedDestination(trimmed);
  if (sizedDestination) {
    const normalized = normalizeMarkdownImageDestination(sizedDestination.path, workspacePath);
    const nextPath = normalized ?? sizedDestination.path.trim();
    return `${nextPath} "width=${sizedDestination.width}"`;
  }
  const titleMatch = trimmed.match(/^(.+?)(\s+["'][^"']*["'])$/);
  const rawPath = (titleMatch ? titleMatch[1] : trimmed).trim();
  const title = titleMatch?.[2] ?? "";
  const unwrapped = rawPath.startsWith("<") && rawPath.endsWith(">") ? rawPath.slice(1, -1).trim() : rawPath;
  const normalized = normalizeMarkdownImageSource(unwrapped, workspacePath);
  if (!normalized || normalized === unwrapped) {
    return null;
  }
  return `${normalized}${title}`;
}

// markdownImageSizedDestination parses Markdown image destinations that carry an explicit width suffix.
// markdownImageSizedDestination 解析携带显式宽度后缀的 Markdown 图片目标。
function markdownImageSizedDestination(destination: string): { path: string; width: number } | null {
  const match = destination.match(/^(.+?)\s+=([1-9]\d{0,4})(?:x\d*)?$/u);
  if (!match) {
    return null;
  }
  return { path: match[1].trim(), width: clampMarkdownImageWidth(Number(match[2])) };
}

// markdownImageWidthFromAlt parses Obsidian-style image alt width hints.
// markdownImageWidthFromAlt 解析 Obsidian 风格的图片 alt 宽度提示。
function markdownImageWidthFromAlt(alt: string): number | null {
  const match = alt.match(/\|\s*([1-9]\d{0,4})\s*$/u);
  return match ? clampMarkdownImageWidth(Number(match[1])) : null;
}

// markdownImageWidthFromTitle parses explicit width hints carried by Markdown image title text.
// markdownImageWidthFromTitle 解析 Markdown 图片 title 文本携带的显式宽度提示。
function markdownImageWidthFromTitle(title: string | undefined): number | null {
  if (!title) {
    return null;
  }
  const match = title.match(/(?:^|\s)(?:width|w)\s*=\s*([1-9]\d{0,4})(?:px)?(?:\s|$)/iu);
  return match ? clampMarkdownImageWidth(Number(match[1])) : null;
}

// clampMarkdownImageWidth caps explicit Markdown image widths to avoid oversized transcript layouts.
// clampMarkdownImageWidth 限制显式 Markdown 图片宽度，避免转录布局被过大图片撑开。
function clampMarkdownImageWidth(width: number): number {
  return Math.min(Math.max(Math.round(width), 1), 800);
}

// looksLikeLocalFilesystemPath detects Windows and Unix filesystem paths carried in Markdown.
// looksLikeLocalFilesystemPath 检测 Markdown 中携带的 Windows 与 Unix 文件系统路径。
function looksLikeLocalFilesystemPath(value: string): boolean {
  const normalized = value.replace(/\\/g, "/");
  return (
    /^[a-zA-Z]:[\\/]/.test(value)
    || value.startsWith("\\\\")
    || value.startsWith("/")
    || normalized.startsWith(".vulcan/code/attachments/")
    || normalized.startsWith("./.vulcan/code/attachments/")
  );
}

// resolveWorkspaceRelativeMarkdownImageSource converts a relative Markdown image path into a project-root media URL.
// resolveWorkspaceRelativeMarkdownImageSource 将 Markdown 相对图片路径转换为项目根目录下的媒体 URL。
function resolveWorkspaceRelativeMarkdownImageSource(value: string, workspacePath?: string | null): string | null {
  const workspaceRoot = workspacePath?.trim();
  if (!workspaceRoot || !looksLikeRelativeMarkdownImagePath(value)) {
    return null;
  }
  const { path, suffix } = splitMarkdownImagePathSuffix(value);
  const relativePath = normalizeRelativeMarkdownImagePath(path);
  if (!relativePath) {
    return null;
  }
  return `${mediaSourceUrl(joinWorkspaceRelativePath(workspaceRoot, relativePath))}${suffix}`;
}

// looksLikeRelativeMarkdownImagePath detects image-only relative links that can be resolved from the workspace root.
// looksLikeRelativeMarkdownImagePath 检测可从工作区根目录解析的图片型相对链接。
function looksLikeRelativeMarkdownImagePath(value: string): boolean {
  const { path } = splitMarkdownImagePathSuffix(value.trim());
  if (!path || looksLikeLocalFilesystemPath(path) || path.startsWith("//")) {
    return false;
  }
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/u.test(path)) {
    return false;
  }
  const normalized = path.replace(/\\/g, "/");
  if (normalized.split("/").some((segment) => segment === "..")) {
    return false;
  }
  return /\.(?:png|jpe?g|webp|gif|bmp|svg)$/iu.test(normalized);
}

// splitMarkdownImagePathSuffix separates a path from query and hash suffixes.
// splitMarkdownImagePathSuffix 将路径与查询、哈希后缀拆分。
function splitMarkdownImagePathSuffix(value: string): { path: string; suffix: string } {
  const match = value.match(/^([^?#]*)([?#].*)?$/u);
  return {
    path: match?.[1]?.trim() ?? value.trim(),
    suffix: match?.[2] ?? "",
  };
}

// normalizeRelativeMarkdownImagePath removes harmless relative prefixes without allowing root escapes.
// normalizeRelativeMarkdownImagePath 移除无害的相对路径前缀，同时避免跳出项目根目录。
function normalizeRelativeMarkdownImagePath(value: string): string | null {
  const normalized = value.replace(/\\/g, "/").replace(/^\.\/+/u, "").replace(/^\/+/u, "").trim();
  if (!normalized || normalized.split("/").some((segment) => segment === "..")) {
    return null;
  }
  return normalized;
}

// joinWorkspaceRelativePath joins one project root and one already-safe relative media path.
// joinWorkspaceRelativePath 拼接项目根目录与已确认安全的相对媒体路径。
function joinWorkspaceRelativePath(workspaceRoot: string, relativePath: string): string {
  const separator = workspaceRoot.includes("\\") ? "\\" : "/";
  const cleanRoot = workspaceRoot.replace(/[\\/]+$/u, "");
  return `${cleanRoot}${separator}${relativePath.replace(/[\\/]+/gu, separator)}`;
}
