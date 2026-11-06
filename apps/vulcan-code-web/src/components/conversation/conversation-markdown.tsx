import React from "react";
import { runConversationControlAction } from "./conversation-control-feedback";
import ReactMarkdown, { defaultUrlTransform, type UrlTransform, type ExtraProps } from "react-markdown";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { Check, Copy, FileText, Globe, Text, TextWrap } from "lucide-react";
import { FileTypeIcon, resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import { remarkLinkLiterals } from "@/components/conversation/conversation-link-literals";
import { MermaidDiagram } from "@/components/conversation/conversation-mermaid";
import { SvgPreview } from "@/components/conversation/conversation-svg-preview";
import type { AnswerComponentHost } from "@/components/conversation/answer-components/answer-component-view";
import { AnswerFrame } from "@/components/conversation/answer-components/answer-frame";
import { answerLocationPath } from "@/components/conversation/answer-components/answer-file-location";
import { markdownTable, numericCells } from "@/components/conversation/answer-components/answer-markdown-table";
import { splitStreamingMarkdownBlocks, STREAMING_MARKDOWN_RAW_TAIL_MAX_CHARS } from "@/components/conversation/streaming-markdown-blocks";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import type { AnswerCodeSnapshot } from "@/service/conversation-event-window-store";
import { MediaPathContextMenu } from "@/components/media/media-path-context-menu";
import {
  highlightedDiffLineSpansForLanguage,
  useHighlightedCodeBlockRevision,
  useDiffShikiTheme,
} from "@/diff/diff-highlight";
import {
  escapeHtml,
  normalizeShikiLanguage,
  shikiLanguageForPath,
  tokenSpansToHtml,
} from "@/diff/diff-shiki-shared";
import { useI18n } from "@/i18n";
import {
  markdownLinkFileTarget,
  opensAsWebPage,
  parseFilePathMention,
  TOP_LEVEL_FILE_LINE_LINK,
  useWorkspaceFileOpener,
  type WorkspaceFileLines,
  type WorkspaceFileOpener,
} from "@/components/workspace/workspace-file-open-request";
import { fileReferenceData, openFileReference } from "@/components/workspace/workspace-file-actions";
import { localMediaSourceUrl } from "@/resources/localMedia";
import { IconButton } from "@/ui-kit/primitives/button";

// MARKDOWN_CODE_HIGHLIGHT_LINE_LIMIT caps Shiki work per fenced Markdown code block.
// MARKDOWN_CODE_HIGHLIGHT_LINE_LIMIT 限制单个 Markdown 代码块触发的 Shiki 高亮行数。
const MARKDOWN_CODE_HIGHLIGHT_LINE_LIMIT = 800;

// MARKDOWN_CODE_HIGHLIGHT_BYTE_LIMIT caps large code blocks before worker highlighting.
// MARKDOWN_CODE_HIGHLIGHT_BYTE_LIMIT 在 Worker 高亮前限制超大代码块体积。
const MARKDOWN_CODE_HIGHLIGHT_BYTE_LIMIT = 240_000;

// LazyAnswerFenceView loads the component parser, renderer and stylesheet only when a reply contains a closed `ui-*` fence.
// LazyAnswerFenceView 仅在回答含已闭合 `ui-*` 围栏时加载组件解析器、渲染器及其样式表。
const LazyAnswerFenceView = React.lazy(async () => {
  const module = await import("@/components/conversation/answer-components/answer-component-view");
  return { default: module.AnswerFenceView };
});

// markdownPlugins stores the Markdown extension set shared by inline and modal renderers; link literals follow GFM
// so they replace its plain-text links.
// markdownPlugins 保存内联与弹窗渲染器共享的 Markdown 扩展集合；链接字面量处理位于 GFM 之后，以取代其普通文本链接。
const markdownPlugins = [remarkGfm, remarkLinkLiterals];

// LazyInlineMath typesets inline `` $`…`$ `` math with KaTeX, which loads only when a reply contains math.
// LazyInlineMath 以 KaTeX 排版行内 `` $`…`$ `` 公式；KaTeX 仅在回答含公式时加载。
const LazyInlineMath = React.lazy(async () => {
  const module = await import("@/components/conversation/answer-components/answer-math");
  return { default: module.MathFormula };
});

// MarkdownHastNode is the part of a HAST node the inline math pass reads and edits.
// MarkdownHastNode 是行内公式处理读取和修改的 HAST 节点部分。
interface MarkdownHastNode {
  type: string;
  tagName?: string;
  value?: string;
  properties?: Record<string, unknown>;
  children?: MarkdownHastNode[];
}

// rehypeInlineMath marks inline code written as `` $`…`$ `` (the GitHub inline math form) as math and drops the
// dollar signs around it. The code span keeps `_` and `*` away from emphasis parsing, and amounts such as
// `$5 and $10` never match. It runs after sanitization, so its marker attribute survives.
// rehypeInlineMath 将写作 `` $`…`$ ``（GitHub 行内公式写法）的行内代码标记为公式并去掉两侧美元符号。
// 代码段使 `_` 与 `*` 不进入强调解析，`$5 and $10` 之类金额也不会匹配。它在清理之后运行，标记属性得以保留。
function rehypeInlineMath() {
  const visit = (node: MarkdownHastNode) => {
    const children = node.children ?? [];
    children.forEach((child, index) => {
      const before = children[index - 1];
      const after = children[index + 1];
      if (
        node.tagName !== "pre" && child.type === "element" && child.tagName === "code"
        && before?.type === "text" && before.value?.endsWith("$") && !before.value.endsWith("$$")
        && after?.type === "text" && after.value?.startsWith("$") && !after.value.startsWith("$$")
      ) {
        before.value = before.value.slice(0, -1);
        after.value = after.value.slice(1);
        child.properties = { ...child.properties, dataMath: "inline" };
      }
      visit(child);
    });
  };
  return (tree: MarkdownHastNode) => visit(tree);
}

// rehypeRootFileLinks makes links to project-root files with lines, such as "make.ps1:3-5", relative ("./make.ps1:3-5"),
// so sanitization does not drop them as unknown schemes. It runs before sanitization.
// rehypeRootFileLinks 将带行的项目根目录文件链接（如 "make.ps1:3-5"）改为相对地址（"./make.ps1:3-5"），使清理不会把它们
// 当作未知协议丢弃。它在清理之前运行。
function rehypeRootFileLinks() {
  const visit = (node: MarkdownHastNode) => {
    const href = node.properties?.href;
    if (node.tagName === "a" && typeof href === "string" && TOP_LEVEL_FILE_LINE_LINK.test(href)) {
      node.properties = { ...node.properties, href: `./${href}` };
    }
    node.children?.forEach(visit);
  };
  return (tree: MarkdownHastNode) => visit(tree);
}

// markdownSanitizers stores the HTML sanitization pipeline for untrusted transcript content.
// markdownSanitizers 保存用于不可信转录内容的 HTML 清理管线。
const markdownSanitizers: NonNullable<React.ComponentProps<typeof ReactMarkdown>["rehypePlugins"]> = [rehypeRootFileLinks, [rehypeSanitize, {
  ...defaultSchema,
  protocols: { ...defaultSchema.protocols, href: [...(defaultSchema.protocols?.href ?? []), "file"] },
}], rehypeInlineMath];

// markdownUrlTransform preserves explicit file hyperlinks while retaining the default policy for all other URLs.
// markdownUrlTransform 保留明确的文件超链接，其余 URL 继续使用默认安全策略。
// url, key and node identify the sanitized attribute; returns a navigable href or the default sanitized URL.
// url、key 和 node 标识已清理的属性；返回可导航 href 或默认清理后的 URL。
const markdownUrlTransform: UrlTransform = (url, key, node) => {
  if (key === "href" && node.tagName === "a" && /^file:\/\//i.test(url)) {
    try {
      return new URL(url).href;
    } catch {
      return "";
    }
  }
  return defaultUrlTransform(url);
};

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

// AnswerCodeSnapshotState tells `ui-code` excerpts whether their reply still streams or which captures it sealed.
// AnswerCodeSnapshotState 告知 `ui-code` 摘录其回答是否仍在流式输出，或已封口的抓取结果。
export type AnswerCodeSnapshotState =
  | { pending: true }
  | { pending: false; snapshots: readonly AnswerCodeSnapshot[] };

// AnswerCodeSnapshotsContext carries the reply's captures to nested `ui-code` fences; null outside replies.
// AnswerCodeSnapshotsContext 把回答的抓取结果传给嵌套 `ui-code` 围栏；回答之外为 null。
const AnswerCodeSnapshotsContext = React.createContext<AnswerCodeSnapshotState | null>(null);

// AnswerCodeSnapshotsProvider scopes one reply's `ui-code` captures to the Markdown rendered inside it.
// AnswerCodeSnapshotsProvider 将单条回答的 `ui-code` 抓取结果限定到其内部渲染的 Markdown。
export function AnswerCodeSnapshotsProvider(props: {
  // Markdown of the reply.
  // 回答的 Markdown。
  children: React.ReactNode;
  // Capture state of the reply, or null when it has none.
  // 回答的抓取状态；没有时为 null。
  value: AnswerCodeSnapshotState | null;
}) {
  return <AnswerCodeSnapshotsContext.Provider value={props.value}>{props.children}</AnswerCodeSnapshotsContext.Provider>;
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
  // Whether fenced code blocks should start in wrapped mode for this Markdown surface.
  // 当前 Markdown 区域中的围栏代码块是否默认启用自动换行。
  defaultCodeWrap?: boolean;
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
    <div className="vk-md">
      {props.streaming ? (
        <StreamingMarkdownMessage
          content={props.content}
          defaultCodeWrap={props.defaultCodeWrap}
          onOpenMedia={props.onOpenMedia}
          workspacePath={workspacePath}
        />
      ) : (
        <LazyMarkdownMessage
          content={finalizedContent}
          defaultCodeWrap={props.defaultCodeWrap}
          eager={props.eager}
          onOpenMedia={props.onOpenMedia}
          workspacePath={workspacePath}
        />
      )}
    </div>
  );
});

// StreamingMarkdownMessage renders a growing body as independent blocks so each update re-parses only changed blocks.
// StreamingMarkdownMessage 将增长中的正文渲染为独立块，使每次更新只重新解析发生变化的块。
function StreamingMarkdownMessage(props: {
  // Raw Markdown content for the active streaming block.
  // 活跃流式块的原始 Markdown 内容。
  content: string;
  // Whether fenced code blocks should start in wrapped mode for this Markdown surface.
  // 当前 Markdown 区域中的围栏代码块是否默认启用自动换行。
  defaultCodeWrap?: boolean;
  // Current project workspace path for image source resolution.
  // 当前项目工作区路径，用于解析图片来源。
  workspacePath: string | null;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  // deferredContent lets urgent interactions interrupt block parsing; React renders only the newest body it reaches.
  // deferredContent 让紧急交互可以打断块解析；React 只渲染它赶上的最新正文。
  const deferredContent = React.useDeferredValue(props.content);
  const blocks = React.useMemo(() => splitStreamingMarkdownBlocks(deferredContent), [deferredContent]);
  // onOpenMediaRef keeps block props stable when the owner recreates its media callback.
  // onOpenMediaRef 在所有者重建媒体回调时保持块属性稳定。
  const onOpenMediaRef = React.useRef(props.onOpenMedia);
  onOpenMediaRef.current = props.onOpenMedia;
  const openMedia = React.useCallback((attachment: GeneratedMediaAttachment) => onOpenMediaRef.current?.(attachment), []);
  return (
    <>
      {blocks.map((block, index) => (
        index === blocks.length - 1 && block.length > STREAMING_MARKDOWN_RAW_TAIL_MAX_CHARS ? (
          <pre className="vk-md-raw" key={index}><code>{block}</code></pre>
        ) : (
          <StreamingMarkdownBlock
            content={block}
            defaultCodeWrap={props.defaultCodeWrap}
            key={index}
            onOpenMedia={props.onOpenMedia ? openMedia : undefined}
            workspacePath={props.workspacePath}
          />
        )
      ))}
    </>
  );
}

// StreamingMarkdownBlock parses one block and is skipped by React while the block's text is unchanged.
// StreamingMarkdownBlock 解析单个块；块文本不变时 React 会跳过它。
const StreamingMarkdownBlock = React.memo(function StreamingMarkdownBlock(props: {
  // Markdown source of one top-level block.
  // 单个顶层块的 Markdown 源码。
  content: string;
  // Whether fenced code blocks should start in wrapped mode for this Markdown surface.
  // 当前 Markdown 区域中的围栏代码块是否默认启用自动换行。
  defaultCodeWrap?: boolean;
  // Current project workspace path for image source resolution.
  // 当前项目工作区路径，用于解析图片来源。
  workspacePath: string | null;
  // Optional stable media opener used by Markdown images.
  // Markdown 图片使用的可选稳定媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  const normalizedContent = React.useMemo(
    () => normalizeMarkdownLocalImageLinks(props.content, props.workspacePath),
    [props.content, props.workspacePath],
  );
  return (
    <MarkdownContentRenderer
      content={normalizedContent}
      defaultCodeWrap={props.defaultCodeWrap}
      onOpenMedia={props.onOpenMedia}
      renderDiagrams={false}
      workspacePath={props.workspacePath}
    />
  );
});

// MarkdownContentRenderer renders one already-normalized Markdown fragment.
// MarkdownContentRenderer 渲染单个已经规范化的 Markdown 片段。
function MarkdownContentRenderer(props: {
  // Markdown fragment to render.
  // 需要渲染的 Markdown 片段。
  content: string;
  // Whether fenced code blocks should start in wrapped mode for this Markdown surface.
  // 当前 Markdown 区域中的围栏代码块是否默认启用自动换行。
  defaultCodeWrap?: boolean;
  // Current project workspace path for image source resolution.
  // 当前项目工作区路径，用于解析图片来源。
  workspacePath: string | null;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
  // Whether finalized Mermaid and SVG fences should render as diagrams.
  // 最终确定的 Mermaid 与 SVG 围栏代码是否应渲染为图表。
  renderDiagrams: boolean;
}) {
  // openFile shows inline file paths in the files panel; null outside the workbench, where paths stay plain code.
  // openFile 在文件面板中显示行内文件路径；工作台之外为 null，路径保持为普通代码。
  const openFile = useWorkspaceFileOpener();
  const components = React.useMemo(() => ({
    a: ({ children, href }: { children?: React.ReactNode; href?: string }) => {
      // Links to project files show the file in the files panel instead of opening a browser tab; links to web pages
      // become file URLs so the desktop opens them in the built-in browser.
      // 指向项目文件的链接在文件面板中显示该文件，而不是打开浏览器标签页；指向网页的链接转为文件 URL，由桌面在内置
      // 浏览器中打开。
      const file = openFile && href ? markdownLinkFileTarget(href) : null;
      const pageHref = file && opensAsWebPage(file.path) ? answerFileHref(file.path, props.workspacePath) : null;
      if (file && openFile && !pageHref) {
        return <MarkdownFileLink lines={file.lines} open={openFile} path={file.path}>{children}</MarkdownFileLink>;
      }
      return (
        <a href={pageHref ?? href} rel="noreferrer" target="_blank">
          {href ? <Globe aria-hidden="true" className="vk-md-link-icon" /> : null}{children}
        </a>
      );
    },
    code: ({ children, className, node }: { children?: React.ReactNode; className?: string } & ExtraProps) => {
      if (node?.properties?.dataMath === "inline" && typeof children === "string") {
        // The source stays visible as code until KaTeX arrives.
        // KaTeX 到达前源码保持为代码显示。
        return (
          <React.Suspense fallback={<code className="vk-md-code">{children}</code>}>
            <LazyInlineMath tex={children} />
          </React.Suspense>
        );
      }
      const inline = !className;
      const mention = inline && openFile && typeof children === "string" ? parseFilePathMention(children) : null;
      const pageHref = mention && opensAsWebPage(mention.path) ? answerFileHref(mention.path, props.workspacePath) : null;
      if (pageHref) return <a href={pageHref} rel="noreferrer" target="_blank"><code className="vk-md-code">{children}</code></a>;
      if (mention && openFile) return <MarkdownFilePath lines={mention.lines} open={openFile} path={mention.path} text={children as string} />;
      return inline ? <code className="vk-md-code">{children}</code> : <code className={className}>{children}</code>;
    },
    img: ({ alt, src, title }: { alt?: string; src?: string; title?: string }) => {
      const width = markdownImageWidthFromTitle(title);
      const imageSource = normalizeMarkdownImageSource(src, props.workspacePath) ?? src;
      const imageAttachment = markdownImageAttachment(imageSource, alt);
      const imageStyle = width ? { width, maxWidth: 800 } : undefined;
      if (imageAttachment && props.onOpenMedia) {
        return (
          <MediaPathContextMenu locator={imageSource}>
          <button
            className="vk-md-image"
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
          </MediaPathContextMenu>
        );
      }
      return (
        <MediaPathContextMenu locator={imageSource}>
          <img
            alt={alt ?? ""}
            decoding="async"
            loading="lazy"
            src={imageSource}
            style={imageStyle}
          />
        </MediaPathContextMenu>
      );
    },
    pre: ({ children, node }: { children?: React.ReactNode } & ExtraProps) => (
      <MarkdownPreBlock
        defaultCodeWrap={props.defaultCodeWrap}
        node={node}
        onOpenMedia={props.onOpenMedia}
        renderDiagrams={props.renderDiagrams}
        workspacePath={props.workspacePath}
      >
        {children}
      </MarkdownPreBlock>
    ),
    table: ({ children, node }: { children?: React.ReactNode } & ExtraProps) => <MarkdownTable node={node}>{children}</MarkdownTable>,
    td: ({ children, node, style }: { children?: React.ReactNode; style?: React.CSSProperties } & ExtraProps) => (
      <MarkdownTableCell node={node} style={style} tag="td">{children}</MarkdownTableCell>
    ),
    th: ({ children, node, style }: { children?: React.ReactNode; style?: React.CSSProperties } & ExtraProps) => (
      <MarkdownTableCell node={node} style={style} tag="th">{children}</MarkdownTableCell>
    ),
  }), [openFile, props.defaultCodeWrap, props.onOpenMedia, props.renderDiagrams, props.workspacePath]);
  return (
    <MarkdownSourceContext.Provider value={props.content}>
      <ReactMarkdown
        components={components}
        rehypePlugins={markdownSanitizers}
        remarkPlugins={markdownPlugins}
        urlTransform={markdownUrlTransform}
      >
        {props.content}
      </ReactMarkdown>
    </MarkdownSourceContext.Provider>
  );
}

// MarkdownFilePath renders inline code that names a file as a control showing that file, at its lines, in the files
// panel. It stays a code element so selection and copying treat it as text.
// MarkdownFilePath 将指名文件的行内代码渲染为在文件面板中显示该文件（及其行）的控件。它仍是 code 元素，
// 选择与复制仍按文本处理。
function MarkdownFilePath(props: { text: string; path: string; lines: WorkspaceFileLines | null; open: WorkspaceFileOpener }) {
  const { t } = useI18n();
  const open = (event?: React.MouseEvent) => openFileReference(props.open, props.path, props.lines, event);
  return (
    <code
      {...fileReferenceData(props.path, props.lines)}
      className="vk-md-code vk-md-path"
      onClick={open}
      onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); open(); } }}
      role="button"
      tabIndex={0}
      title={t("workspaceFiles.showInFilesPanel")}
    >
      {props.text}
    </code>
  );
}

// MarkdownFileLink renders a Markdown link to a project file as a control styled like a link that shows the file,
// at its lines, in the files panel; its tooltip names the file. A button, so no click can open the relative address
// as a broken browser tab.
// MarkdownFileLink 将指向项目文件的 Markdown 链接渲染为链接样式的控件，在文件面板中显示该文件（及其行）；其提示指明
// 该文件。使用按钮，任何点击都不会把相对地址作为失效的浏览器标签页打开。
function MarkdownFileLink(props: { children?: React.ReactNode; path: string; lines: WorkspaceFileLines | null; open: WorkspaceFileOpener }) {
  const { t } = useI18n();
  return (
    <button
      {...fileReferenceData(props.path, props.lines)}
      className="vk-md-file-link"
      onClick={(event) => openFileReference(props.open, props.path, props.lines, event)}
      title={props.path + " · " + t("workspaceFiles.showInFilesPanel")}
      type="button"
    >
      <FileText aria-hidden="true" className="vk-md-link-icon" />{props.children}
    </button>
  );
}

// MarkdownSourceContext carries the exact Markdown text a renderer parsed, so fences can be checked against it.
// MarkdownSourceContext 携带渲染器解析的准确 Markdown 文本，使围栏可以据此校验。
const MarkdownSourceContext = React.createContext("");

// MarkdownTableColumnsContext maps each table cell node to whether its column holds only numbers.
// MarkdownTableColumnsContext 将每个表格单元格节点映射到其所在列是否只含数字。
const MarkdownTableColumnsContext = React.createContext<WeakMap<object, boolean> | null>(null);

// hastText concatenates the text content of one sanitized HTML AST node.
// hastText 拼接单个已清理 HTML AST 节点的文本内容。
function hastText(node: unknown): string {
  const value = node as { type?: string; value?: string; children?: unknown[] };
  if (value.type === "text") return value.value ?? "";
  return (value.children ?? []).map(hastText).join("");
}

// hastElements returns the element children of one AST node with the given tag names.
// hastElements 返回单个 AST 节点中具有指定标签名的元素子节点。
function hastElements(node: unknown, tags: readonly string[]): ExtraProps["node"][] {
  const children = (node as { children?: unknown[] } | undefined)?.children ?? [];
  return children.filter((child): child is NonNullable<ExtraProps["node"]> => (
    (child as { type?: string }).type === "element" && tags.includes((child as { tagName: string }).tagName)
  ));
}

// MarkdownTable frames a GFM table, right-aligns numeric columns and copies the table back as Markdown.
// MarkdownTable 为 GFM 表格加外框，数字列靠右，并可复制回 Markdown。
function MarkdownTable(props: { children?: React.ReactNode; node?: ExtraProps["node"] }) {
  const { copy, numeric } = React.useMemo(() => {
    const rows = hastElements(props.node, ["thead", "tbody"]).flatMap((section) => hastElements(section, ["tr"]));
    const cells = rows.map((row) => hastElements(row, ["th", "td"]));
    const text = cells.map((row) => row.map(hastText));
    const columns = new WeakMap<object, boolean>();
    const width = Math.max(0, ...text.map((row) => row.length));
    for (let column = 0; column < width; column += 1) {
      const isNumeric = numericCells(text.slice(1).map((row) => row[column] ?? ""));
      for (const row of cells) if (row[column]) columns.set(row[column]!, isNumeric);
    }
    const [header = [], ...body] = text;
    return { copy: markdownTable(header, body), numeric: columns };
  }, [props.node]);
  return (
    <MarkdownTableColumnsContext.Provider value={numeric}>
      <AnswerFrame copy={copy} host={markdownCopyHost}>
        <div className="vk-table-wrap"><table className="vk-table">{props.children}</table></div>
      </AnswerFrame>
    </MarkdownTableColumnsContext.Provider>
  );
}

// MarkdownTableCell marks numeric columns unless the author set an explicit alignment.
// MarkdownTableCell 标记数字列，作者显式指定对齐时除外。
function MarkdownTableCell(props: { children?: React.ReactNode; node?: ExtraProps["node"]; style?: React.CSSProperties; tag: "td" | "th" }) {
  const columns = React.useContext(MarkdownTableColumnsContext);
  const numeric = !props.style?.textAlign && props.node ? columns?.get(props.node) : undefined;
  const Tag = props.tag;
  return <Tag data-numeric={numeric || undefined} style={props.style}>{props.children}</Tag>;
}

// markdownCopyHost lends the shared clipboard path to framed Markdown blocks.
// markdownCopyHost 为框架型 Markdown 区块提供共享剪贴板路径。
const markdownCopyHost = { copy: copyMarkdownCode };

// LazyMarkdownMessage keeps one complete Markdown document mounted for stable layout.
// LazyMarkdownMessage 保持完整 Markdown 文档挂载以获得稳定布局。
function LazyMarkdownMessage(props: {
  // Already-normalized Markdown content.
  // 已经规范化的 Markdown 内容。
  content: string;
  // Whether fenced code blocks should start in wrapped mode for this Markdown surface.
  // 当前 Markdown 区域中的围栏代码块是否默认启用自动换行。
  defaultCodeWrap?: boolean;
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
      defaultCodeWrap={props.defaultCodeWrap}
      onOpenMedia={props.onOpenMedia}
      renderDiagrams
      workspacePath={props.workspacePath}
    />
  );
}

// MarkdownPreBlock routes finalized Mermaid and SVG fences to diagram renderers and preserves other code blocks.
// MarkdownPreBlock 将最终 Mermaid 与 SVG 围栏路由到图表渲染器，并保留其他代码块。
//
// Parameters:
// 参数：
// - props: ReactMarkdown code children and the finalized-render permission.
// - props：ReactMarkdown 代码子节点与最终渲染许可。
//
// Returns:
// 返回值：
// A rendered diagram or the standard fenced-code presentation.
// 渲染后的图表或标准围栏代码展示。
function MarkdownPreBlock(props: {
  // Existing sanitized AST node and optional canonical source context.
  // 既有已清理 AST 节点及可选权威源码上下文。
  node?: ExtraProps["node"];
  // ReactMarkdown code element children wrapped by the pre component.
  // ReactMarkdown 中由 pre 包裹的 code 元素子节点。
  children?: React.ReactNode;
  // Whether fenced code blocks should start in wrapped mode for this Markdown surface.
  // 当前 Markdown 区域中的围栏代码块是否默认启用自动换行。
  defaultCodeWrap?: boolean;
  // Whether diagram rendering is allowed for this stable Markdown document.
  // 当前稳定 Markdown 文档是否允许图表渲染。
  renderDiagrams: boolean;
  // Current project workspace path for component file links and media.
  // 当前项目工作区路径，用于组件文件链接与媒体。
  workspacePath: string | null;
  // Optional media opener used by gallery components.
  // 图库组件使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  // source is the Markdown text whose AST produced this block.
  // source 是生成该块 AST 的 Markdown 文本。
  const source = React.useContext(MarkdownSourceContext);
  // codeElement is the fenced code node emitted by ReactMarkdown.
  // codeElement 是 ReactMarkdown 生成的围栏代码节点。
  const codeElement = markdownCodeElement(props.children);
  // language is the normalized information string attached to the fenced block.
  // language 是围栏代码块携带的规范化信息字符串。
  const language = markdownCodeLanguage(codeElement?.props.className)?.toLowerCase();
  // fence is the raw `ui-*` fence once closed; streaming fences stay code, and the deferred renderer parses it.
  // fence 是已闭合的原始 `ui-*` 围栏；流式围栏保持为代码，由延迟渲染器解析。
  const fence = React.useMemo(() => {
    if (!language?.startsWith("ui-") || !closedFence(source, props.node)) return null;
    const code = hastElements(props.node, ["code"])[0];
    const meta = (code?.data as { meta?: unknown } | undefined)?.meta;
    const body = markdownCodeText(codeElement?.props.children ?? props.children).replace(/\n$/u, "");
    return { language, meta: typeof meta === "string" ? meta : "", body };
  }, [codeElement, language, props.children, props.node, source]);
  // codeSnapshots are the reply's `ui-code` captures, read by excerpts at any nesting depth.
  // codeSnapshots 是回答的 `ui-code` 抓取结果，任意嵌套深度的摘录都会读取。
  const codeSnapshots = React.useContext(AnswerCodeSnapshotsContext);
  // openFile lets component file references show the file in the files panel inside the workbench.
  // openFile 使组件中的文件引用在工作台内于文件面板中显示该文件。
  const openFile = useWorkspaceFileOpener();
  // host lends nested Markdown, media, file links, captured code and the clipboard to the component.
  // host 为组件提供嵌套 Markdown、媒体、文件链接、抓取的代码与剪贴板。
  const host = React.useMemo<AnswerComponentHost>(() => ({
    renderMarkdown: (body) => (
      <MarkdownContentRenderer
        content={normalizeMarkdownLocalImageLinks(body, props.workspacePath)}
        defaultCodeWrap={props.defaultCodeWrap}
        onOpenMedia={props.onOpenMedia}
        renderDiagrams={props.renderDiagrams}
        workspacePath={props.workspacePath}
      />
    ),
    mediaSource: (path) => answerMediaSource(path, props.workspacePath),
    fileHref: (location) => answerFileHref(location, props.workspacePath),
    openFile: openFile ?? undefined,
    codeSnapshots,
    renderCode: (excerpt) => (
      <CodeBlockFigure
        code={excerpt.code}
        defaultCodeWrap={props.defaultCodeWrap}
        firstLineNumber={excerpt.firstLine}
        header={excerpt.header}
        highlightLanguage={normalizeShikiLanguage(shikiLanguageForPath(excerpt.path))}
        highlightedLines={excerpt.highlighted}
      />
    ),
    copy: copyMarkdownCode,
    onOpenMedia: props.onOpenMedia,
  }), [codeSnapshots, openFile, props.defaultCodeWrap, props.onOpenMedia, props.renderDiagrams, props.workspacePath]);
  if (fence) {
    // The fence keeps its code presentation until the deferred renderer arrives, and for good when it is invalid.
    // 延迟渲染器到达前围栏保持代码展示；语法无效时始终如此。
    const codeBlock = <MarkdownCodeBlock defaultCodeWrap={props.defaultCodeWrap}>{props.children}</MarkdownCodeBlock>;
    return (
      <React.Suspense fallback={codeBlock}>
        <LazyAnswerFenceView {...fence} fallback={codeBlock} host={host} />
      </React.Suspense>
    );
  }
  if (props.renderDiagrams && (language === "mermaid" || language === "svg")) {
    // source is the complete finalized diagram definition without ReactMarkdown's trailing newline.
    // source 是移除 ReactMarkdown 尾随换行后的完整最终图表定义。
    const source = markdownCodeText(codeElement?.props.children ?? props.children).replace(/\n$/u, "");
    // fallback preserves the ordinary fenced-code presentation when preview rendering fails.
    // fallback 在预览渲染失败时保留普通围栏代码展示。
    const fallback = <MarkdownCodeBlock defaultCodeWrap={props.defaultCodeWrap}>{props.children}</MarkdownCodeBlock>;
    return language === "mermaid" ? (
      <MermaidDiagram
        copySource={() => copyMarkdownCode(source)}
        fallback={fallback}
        source={source}
      />
    ) : (
      <SvgPreview
        copySource={() => copyMarkdownCode(source)}
        fallback={fallback}
        source={source}
      />
    );
  }
  return <MarkdownCodeBlock defaultCodeWrap={props.defaultCodeWrap}>{props.children}</MarkdownCodeBlock>;
}

// MarkdownCodeBlock renders fenced code with language chrome, copy, wrap, and syntax highlighting.
// MarkdownCodeBlock 为围栏代码渲染语言栏、复制、折行与语法高亮。
function MarkdownCodeBlock(props: {
  // ReactMarkdown code element children wrapped by the pre component.
  // ReactMarkdown 中由 pre 包裹的 code 元素子节点。
  children?: React.ReactNode;
  // Whether the wrap control starts enabled for this code block.
  // 该代码块的换行控制是否默认启用。
  defaultCodeWrap?: boolean;
}) {
  const codeElement = markdownCodeElement(props.children);
  const className = codeElement?.props.className;
  const rawCode = markdownCodeText(codeElement?.props.children ?? props.children);
  const code = rawCode.replace(/\n$/u, "");
  const language = markdownCodeLanguage(className);
  const displayLanguage = markdownCodeDisplayLanguage(language);
  const icon = React.useMemo(
    () => resolveFileTypeIcon(markdownCodeIconHint(displayLanguage)),
    [displayLanguage],
  );
  return (
    <CodeBlockFigure
      code={code}
      defaultCodeWrap={props.defaultCodeWrap}
      header={(
        <span className="vk-codeblock-lang">
          <FileTypeIcon icon={icon} />
          <span>{displayLanguage}</span>
        </span>
      )}
      highlightLanguage={normalizeShikiLanguage(markdownCodeHighlightLanguage(language))}
    />
  );
}

// CodeBlockFigure renders code lines under a header with copy and wrap actions, syntax highlighting and an
// optional line-number gutter whose highlighted lines are marked.
// CodeBlockFigure 在带复制与折行操作的标题下渲染代码行，提供语法高亮与可选行号栏，并标记高亮行。
function CodeBlockFigure(props: {
  // Source text without a trailing newline.
  // 不含尾随换行的源码文本。
  code: string;
  // Left side of the header: the language label or a file location.
  // 标题左侧内容：语言标签或文件位置。
  header: React.ReactNode;
  // Canonical Shiki language, or null for plain text.
  // 规范 Shiki 语言；纯文本时为 null。
  highlightLanguage: string | null;
  // Whether the wrap control starts enabled for this code block.
  // 该代码块的换行控制是否默认启用。
  defaultCodeWrap?: boolean;
  // Number of the first line; a gutter is shown only when present.
  // 首行行号；仅在提供时显示行号栏。
  firstLineNumber?: number;
  // Line numbers drawn with the highlight background.
  // 以高亮背景绘制的行号。
  highlightedLines?: ReadonlySet<number>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const { code, firstLineNumber, highlightLanguage, highlightedLines } = props;
  const lines = React.useMemo(() => code.split(/\r\n|\r|\n/u), [code]);
  const highlightEnabled = Boolean(
    highlightLanguage
    && lines.length <= MARKDOWN_CODE_HIGHLIGHT_LINE_LIMIT
    && code.length <= MARKDOWN_CODE_HIGHLIGHT_BYTE_LIMIT,
  );
  const shikiTheme = useDiffShikiTheme();
  const highlightRevision = useHighlightedCodeBlockRevision(lines, highlightLanguage, highlightEnabled);
  const renderedLines = React.useMemo(
    () => markdownCodeRenderedLines(lines, highlightLanguage, highlightRevision, highlightEnabled),
    [highlightEnabled, highlightRevision, lines, highlightLanguage, shikiTheme],
  );
  const [copied, setCopied] = React.useState(false);
  const [wrapped, setWrapped] = React.useState(props.defaultCodeWrap ?? false);

  React.useEffect(() => {
    if (!copied) {
      return undefined;
    }
    const timer = window.setTimeout(() => setCopied(false), 1200);
    return () => window.clearTimeout(timer);
  }, [copied]);

  return (
    <figure className="vk-codeblock">
      <figcaption className="vk-codeblock-head">
        {props.header}
        <span className="vk-codeblock-actions">
          <IconButton
            aria-label={wrapped ? t("markdown.wrap.disable") : t("markdown.wrap.enable")}
            className="vk-codeblock-action" size="xs"
            onClick={() => setWrapped((current) => !current)}
            title={wrapped ? t("markdown.wrap.disable") : t("markdown.wrap.enable")}
            type="button"
          >
            {wrapped ? <Text aria-hidden="true" /> : <TextWrap aria-hidden="true" />}
          </IconButton>
          <IconButton
            aria-label={copied ? t("markdown.copied") : t("markdown.copyCode")}
            className="vk-codeblock-action" size="xs"
            onClick={() => {
              setCopied(false);
              void runConversationControlAction(
                () => copyMarkdownCode(code), t("conversation.control.copyFailed"),
              ).then((success) => setCopied(success));
            }}
            title={copied ? t("markdown.copied") : t("markdown.copyCode")}
            type="button"
          >
            {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
          </IconButton>
        </span>
      </figcaption>
      <div className="vk-codeblock-scroll" data-wrap={wrapped ? "true" : "false"}>
        <pre className="vk-codeblock-pre" data-numbered={firstLineNumber === undefined ? undefined : "true"}>
          <code>
            {renderedLines.map((line, index) => {
              const html = { __html: line || "&#8203;" };
              if (firstLineNumber === undefined) {
                return <span className="vk-codeblock-line" dangerouslySetInnerHTML={html} key={`${index}:${line.length}`} />;
              }
              const number = firstLineNumber + index;
              return (
                <span
                  className="vk-codeblock-line"
                  data-highlighted={highlightedLines?.has(number) || undefined}
                  key={`${index}:${line.length}`}
                >
                  <span aria-hidden="true" className="vk-codeblock-num">{number}</span>
                  <span dangerouslySetInnerHTML={html} />
                </span>
              );
            })}
          </code>
        </pre>
      </div>
    </figure>
  );
}

// markdownCodeElement returns the single code element carried by a pre node when available.
// markdownCodeElement 在可用时返回 pre 节点内携带的单个 code 元素。
function markdownCodeElement(children: React.ReactNode): React.ReactElement<{ className?: string; children?: React.ReactNode }> | null {
  if (React.isValidElement<{ className?: string; children?: React.ReactNode }>(children)) {
    return children;
  }
  if (Array.isArray(children) && children.length === 1 && React.isValidElement<{ className?: string; children?: React.ReactNode }>(children[0])) {
    return children[0];
  }
  return null;
}

// markdownCodeText converts nested Markdown code children into a stable source string.
// markdownCodeText 将嵌套的 Markdown 代码子节点转换为稳定源码字符串。
function markdownCodeText(children: React.ReactNode): string {
  if (children == null || typeof children === "boolean") {
    return "";
  }
  if (typeof children === "string" || typeof children === "number") {
    return String(children);
  }
  if (Array.isArray(children)) {
    return children.map(markdownCodeText).join("");
  }
  if (React.isValidElement<{ children?: React.ReactNode }>(children)) {
    return markdownCodeText(children.props.children);
  }
  return "";
}

// markdownCodeLanguage extracts the fence language from a react-markdown class name.
// markdownCodeLanguage 从 react-markdown 的 className 中提取围栏语言。
function markdownCodeLanguage(className: string | undefined): string | null {
  const match = className?.match(/(?:^|\s)language-([^\s]+)/u);
  return match?.[1]?.trim() || null;
}

// markdownCodeDisplayLanguage returns the compact language label shown in the code header.
// markdownCodeDisplayLanguage 返回代码块头部显示的紧凑语言标签。
function markdownCodeDisplayLanguage(language: string | null): string {
  const normalized = language?.toLowerCase() ?? "";
  return MARKDOWN_CODE_LANGUAGE_LABELS[normalized] ?? (normalized || "text");
}

// markdownCodeHighlightLanguage maps Markdown fence aliases to Shiki language ids.
// markdownCodeHighlightLanguage 将 Markdown 围栏别名映射为 Shiki 语言标识。
function markdownCodeHighlightLanguage(language: string | null): string | null {
  const normalized = language?.toLowerCase() ?? "";
  return MARKDOWN_CODE_HIGHLIGHT_LANGUAGE_ALIASES[normalized] ?? (normalized || null);
}

// markdownCodeIconHint converts a language label into a pseudo file name for the shared icon resolver.
// markdownCodeIconHint 将语言标签转换为共享图标解析器可识别的伪文件名。
function markdownCodeIconHint(language: string): string {
  return MARKDOWN_CODE_ICON_HINTS[language.toLowerCase()] ?? `snippet.${language}`;
}

// markdownCodeRenderedLines renders highlighted line HTML when cached, otherwise escaped source.
// markdownCodeRenderedLines 在缓存可用时渲染高亮行 HTML，否则渲染转义源码。
function markdownCodeRenderedLines(
  lines: readonly string[],
  language: string | null,
  _highlightRevision: number,
  highlightEnabled: boolean,
): string[] {
  return lines.map((line) => {
    const spans = highlightEnabled && language ? highlightedDiffLineSpansForLanguage(line, language) : null;
    return spans ? tokenSpansToHtml(spans) : escapeHtml(line);
  });
}

// copyMarkdownCode writes a Markdown code block to the clipboard with a textarea fallback.
// copyMarkdownCode 将 Markdown 代码块写入剪贴板，并提供 textarea 降级。
async function copyMarkdownCode(code: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(code);
      return;
    } catch {
      // Fall back to the legacy selection path when the Clipboard API is blocked.
      // 当 Clipboard API 被阻止时降级到旧版选区复制路径。
    }
  }
  const textarea = document.createElement("textarea");
  textarea.value = code;
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  try {
    textarea.select();
    // The legacy browser API reports denial as false instead of rejecting a promise.
    // 旧浏览器接口通过 false 报告拒绝，而不是拒绝 Promise。
    if (!document.execCommand("copy")) throw new Error("Clipboard copy was rejected.");
  } finally {
    textarea.remove();
  }
}

const MARKDOWN_CODE_LANGUAGE_LABELS: Record<string, string> = {
  js: "javascript",
  jsx: "jsx",
  shell: "bash",
  sh: "bash",
  text: "text",
  txt: "text",
  plaintext: "text",
  ts: "typescript",
  tsx: "tsx",
};

const MARKDOWN_CODE_HIGHLIGHT_LANGUAGE_ALIASES: Record<string, string> = {
  javascript: "javascript",
  js: "javascript",
  shell: "bash",
  sh: "bash",
  text: "text",
  txt: "text",
  plaintext: "text",
  typescript: "typescript",
  ts: "typescript",
};

const MARKDOWN_CODE_ICON_HINTS: Record<string, string> = {
  bash: "snippet.sh",
  javascript: "snippet.js",
  jsx: "snippet.jsx",
  lua: "snippet.lua",
  powershell: "snippet.ps1",
  python: "snippet.py",
  text: "snippet.txt",
  typescript: "snippet.ts",
  tsx: "snippet.tsx",
};

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
  if (looksLikeAbsoluteLocalFilesystemPath(unwrapped)) {
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
// Image syntax inside fenced blocks or inline code spans is literal text and stays untouched.
// 围栏代码块或行内代码中的图片语法是字面文本，保持不变。
function normalizeMarkdownLocalImageLinks(content: string, workspacePath?: string | null): string {
  const codeRanges = markdownCodeRanges(content);
  return content.replace(/!\[([^\]\n]*)\]\(([^)\n]+)\)/g, (full, alt: string, destination: string, offset: number) => {
    if (codeRanges.some(([start, end]) => offset >= start && offset < end)) {
      return full;
    }
    const normalized = normalizeMarkdownImageDestination(destination, workspacePath);
    const width = markdownImageWidthFromAlt(alt);
    const cleanAlt = width ? alt.replace(/\|\s*\d+\s*$/u, "").trim() : alt;
    const nextDestination = normalized ?? destination.trim();
    const widthTitle = width ? ` "width=${width}"` : "";
    const imageMarkdown = normalized || width ? `![${cleanAlt}](${nextDestination}${widthTitle})` : full;
    return `${markdownImageLeadingBlockPrefix(content, offset)}${imageMarkdown}`;
  });
}

// markdownCodeRanges returns [start, end) offsets of fenced code blocks and inline code spans.
// markdownCodeRanges 返回围栏代码块与行内代码的 [start, end) 偏移区间。
// An unclosed fence runs to the end of the content, matching how CommonMark renders it.
// 未闭合的围栏延伸到正文末尾，与 CommonMark 的渲染方式一致。
function markdownCodeRanges(content: string): Array<[number, number]> {
  const ranges: Array<[number, number]> = [];
  let fence: { marker: string; length: number; start: number } | null = null;
  let proseStart = 0;
  let lineStart = 0;
  const addInlineSpans = (from: number, to: number) => {
    const text = content.slice(from, to);
    const runs = [...text.matchAll(/`+/gu)];
    for (let index = 0; index < runs.length; index += 1) {
      const opening = runs[index];
      const closingIndex = runs.findIndex((run, candidate) => candidate > index && run[0].length === opening[0].length);
      if (closingIndex < 0) {
        continue;
      }
      const closing = runs[closingIndex];
      ranges.push([from + (opening.index ?? 0), from + (closing.index ?? 0) + closing[0].length]);
      index = closingIndex;
    }
  };
  while (lineStart <= content.length) {
    const newline = content.indexOf("\n", lineStart);
    const lineEnd = newline < 0 ? content.length : newline + 1;
    const line = content.slice(lineStart, lineEnd);
    const marker = /^ {0,3}(`{3,}|~{3,})/u.exec(line)?.[1];
    if (fence) {
      if (marker && marker[0] === fence.marker && marker.length >= fence.length && line.trim() === marker) {
        ranges.push([fence.start, lineEnd]);
        fence = null;
        proseStart = lineEnd;
      }
    } else if (marker && !(marker[0] === "`" && line.slice(line.indexOf(marker) + marker.length).includes("`"))) {
      addInlineSpans(proseStart, lineStart);
      fence = { marker: marker[0], length: marker.length, start: lineStart };
    }
    if (newline < 0) {
      break;
    }
    lineStart = lineEnd;
  }
  if (fence) {
    ranges.push([fence.start, content.length]);
  } else {
    addInlineSpans(proseStart, content.length);
  }
  return ranges;
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

// looksLikeAbsoluteLocalFilesystemPath detects absolute Windows and Unix filesystem paths carried in Markdown.
// looksLikeAbsoluteLocalFilesystemPath 检测 Markdown 中携带的 Windows 与 Unix 绝对文件系统路径。
function looksLikeAbsoluteLocalFilesystemPath(value: string): boolean {
  return (
    /^[a-zA-Z]:[\\/]/.test(value)
    || value.startsWith("\\\\")
    || value.startsWith("/")
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
  if (!path || looksLikeAbsoluteLocalFilesystemPath(path) || path.startsWith("//")) {
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

// closedFence reports whether a fenced block's source ends with a closing fence at least as long as its opening.
// closedFence 判断围栏块源码是否以不短于起始围栏的闭合围栏结束。
// Unclosed fences (still streaming) end at the end of the document instead. 未闭合（仍在流式输出）的围栏则止于文档末尾。
function closedFence(source: string, node: ExtraProps["node"]): boolean {
  const start = node?.position?.start.offset;
  const end = node?.position?.end.offset;
  if (start === undefined || end === undefined) return false;
  // Container prefixes (list indentation, quote markers) precede both fence lines.
  // 容器前缀（列表缩进、引用标记）位于两条围栏行之前。
  const lines = source.slice(start, end).split(/\r?\n/u).map((line) => line.replace(/^[\s>]*/u, "").trimEnd());
  const opening = /^(`{3,}|~{3,})/u.exec(lines[0] ?? "");
  const closing = lines[lines.length - 1];
  return lines.length >= 2 && opening !== null && closing.length >= opening[1].length
    && closing === opening[1][0].repeat(closing.length);
}

// answerMediaSource resolves a gallery path to a renderable URL; relative paths need the project root.
// answerMediaSource 将图库路径解析为可渲染 URL；相对路径需要项目根目录。
function answerMediaSource(path: string, workspacePath: string | null): string | null {
  const value = path.trim();
  if (/^(https?:|data:(image|video)\/)/iu.test(value)) return value;
  if (/^(file:|vulcan-media:)/iu.test(value) || looksLikeAbsoluteLocalFilesystemPath(value)) return mediaSourceUrl(value);
  if (/^[a-z][a-z0-9+.-]*:/iu.test(value) || !workspacePath) return null;
  const relativePath = normalizeRelativeMarkdownImagePath(value);
  return relativePath ? mediaSourceUrl(joinWorkspaceRelativePath(workspacePath, relativePath)) : null;
}

// answerFileHref links a file reference such as `src/a.rs:42` to its file URL; the line suffix is display-only.
// answerFileHref 将 `src/a.rs:42` 这类文件引用链接到文件 URL；行号后缀仅用于显示。
function answerFileHref(location: string, workspacePath: string | null): string | null {
  const path = answerLocationPath(location);
  if (path === null) return null;
  let absolute: string | null = null;
  if (looksLikeAbsoluteLocalFilesystemPath(path)) {
    absolute = path;
  } else if (workspacePath && !/^[a-z][a-z0-9+.-]*:/iu.test(path)) {
    const relativePath = normalizeRelativeMarkdownImagePath(path);
    absolute = relativePath ? joinWorkspaceRelativePath(workspacePath, relativePath) : null;
  }
  if (absolute === null) return null;
  try {
    return new URL(`file:///${absolute.replace(/\\/gu, "/").replace(/^\/+/u, "")}`).href;
  } catch {
    return null;
  }
}
