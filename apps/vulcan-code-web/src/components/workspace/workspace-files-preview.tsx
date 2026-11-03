import React from "react";
import { FileText } from "lucide-react";
import {
  MarkdownImageWorkspacePathProvider,
  MarkdownMessage,
} from "@/components/conversation/conversation-markdown";
import {
  diffLanguageForFile,
  highlightedDiffLineSpansForDiffLine,
  useHighlightedDiffFileRevision,
  useHighlightLanguage,
} from "@/diff/diff-highlight";
import { escapeHtml, tokenSpansToHtml } from "@/diff/diff-shiki-shared";
import type { DiffFile, DiffLine } from "@/diff/types";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { MediaPathContextMenu } from "@/components/media/media-path-context-menu";
import { requestComposerCommand } from "@/components/composer/composer-external-commands";
import { ConversationFindBar } from "@/components/conversation/conversation-find-bar";
import { ConversationSelectionQuote } from "@/components/conversation/conversation-selection-quote";
import { codeQuoteMarkdown, previewSelectionLines, selectPreviewLines } from "@/components/workspace/workspace-file-preview-quote";
import {
  consumeWorkspaceFileLineReveal,
  peekWorkspaceFileLineReveal,
  type WorkspaceFileLines,
} from "@/components/workspace/workspace-file-open-request";
import { useI18n, type TranslationKey } from "@/i18n";
import type { WorkspaceFileEntrySummary, WorkspaceFileReadResponse } from "@/types";
import { useWorkspaceFileWrapLines } from "@/components/workspace/workspace-file-wrap-lines";
import { DocumentViewer } from "@/components/file-viewers/document-viewer";
import { fileViewerKind } from "@/components/file-viewers/file-viewer-kind";

// workspaceFilePreviewMaxHighlightRows caps Shiki preview work inside the right rail.
// workspaceFilePreviewMaxHighlightRows 限制右侧栏内 Shiki 预览工作量。
const workspaceFilePreviewMaxHighlightRows = 3_000;

// WorkspaceFilePreview renders the selected file preview pane.
// WorkspaceFilePreview 渲染当前选中文件预览区域。
// Parameters: props supplies the selected preview, loading/error state and retry/permission callbacks; returns the matching code, media or explicit unavailable surface.
// 参数：props 提供选中预览、加载和错误状态及重试和许可回调；返回对应代码、媒体或明确不可用界面。
export function WorkspaceFilePreview(props: {
  // Selected file entry.
  // 当前选中文件条目。
  entry: WorkspaceFileEntrySummary | null;
  // Selected file path when its entry can no longer be resolved.
  // 无法再解析已选中文件条目时的选中文件路径。
  missingPath: string | null;
  // Loaded preview response.
  // 已加载预览响应。
  preview: WorkspaceFileReadResponse | null;
  // Selected media file waiting for display permission.
  // 等待展示许可的已选中媒体文件。
  mediaPermissionEntry: WorkspaceFileEntrySummary | null;
  // Whether a preview request is running.
  // 预览请求是否正在运行。
  loading: boolean;
  // Latest preview error.
  // 最近一次预览错误。
  error: string | null;
  // Resolves media file paths into renderable URLs.
  // 将媒体文件路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Grants media display permission and opens the selected media file.
  // 授权媒体展示并打开已选中的媒体文件。
  onAllowMediaPreview: (entry: WorkspaceFileEntrySummary) => void;
  // Reloads the exact selected file after a failed read.
  // 读取失败后重新加载精确选中的文件。
  onRetryPreview: () => void;
}) {
  const { t } = useI18n();
  if (!props.entry) {
    if (props.missingPath) {
      return (
        <WorkspaceMissingFilePreview
          missingPath={props.missingPath}
        />
      );
    }
    return (
      <div className="workspace-file-preview empty">
        <FileText size={24} />
        <strong>{t("workspaceFilePreview.openFile")}</strong>
        <span>{t("workspaceFilePreview.emptyHint")}</span>
      </div>
    );
  }
  if (props.loading) {
    return <div className="workspace-file-preview"><StreamPlaceholder className="vk-stream-inset" label={t("workspaceFilePreview.loading")} count={8} /></div>;
  }
  if (props.error) {
    return <div className="workspace-file-preview empty error" role="alert">
      <span>{props.error}</span>
      <Button size="sm" variant="outline" onClick={props.onRetryPreview}>{t("workspaceAudit.retry")}</Button>
    </div>;
  }
  if (props.mediaPermissionEntry && props.entry.path === props.mediaPermissionEntry.path) {
    return (
      <WorkspaceMediaPermissionPrompt
        entry={props.mediaPermissionEntry}
        onAllowMediaPreview={props.onAllowMediaPreview}
      />
    );
  }
  if (!props.preview) {
    if (props.entry.kind === "other") {
      return (
        <div className="workspace-file-preview empty">
          <FileText size={24} />
          <strong>{t("workspaceFilePreview.unsupportedTitle")}</strong>
          <span>{t("workspaceFilePreview.unsupportedHint")}</span>
        </div>
      );
    }
    return <div className="workspace-file-preview empty">{t("workspaceFilePreview.noContent")}</div>;
  }
  if (props.preview.kind === "image") {
    return (
      <div className="workspace-file-preview media">
        <MediaPathContextMenu locator={props.preview.path}>
          <img alt={props.preview.relativePath} decoding="async" src={props.resolveMediaSourceUrl(props.preview.path)} />
        </MediaPathContextMenu>
      </div>
    );
  }
  if (props.preview.kind === "video") {
    return (
      <div className="workspace-file-preview media">
        <MediaPathContextMenu locator={props.preview.path}>
          <video aria-label={props.preview.relativePath} controls preload="metadata" src={props.resolveMediaSourceUrl(props.preview.path)} />
        </MediaPathContextMenu>
      </div>
    );
  }
  const documentKind = props.preview.kind === "document" ? fileViewerKind(props.preview.path) : null;
  if (documentKind) {
    return (
      <div className="workspace-file-preview document">
        <DocumentViewer kind={documentKind} name={props.preview.relativePath} path={props.preview.path} />
      </div>
    );
  }
  return <WorkspaceTextPreview markdown={props.entry.extension === "md"} preview={props.preview} />;
}

// WorkspaceTextPreview shows a loaded text preview: Markdown as it reads, anything else as highlighted code.
// WorkspaceTextPreview 显示已加载的文本预览：Markdown 按阅读形态显示，其他内容按高亮代码显示。
export function WorkspaceTextPreview(props: { preview: WorkspaceFileReadResponse; markdown: boolean }) {
  if (props.markdown) {
    return <WorkspaceMarkdownPreview preview={props.preview} />;
  }
  return (
    <div className="workspace-file-preview code" data-find-scope="">
      <WorkspaceHighlightedCodePreview preview={props.preview} />
    </div>
  );
}

// WorkspaceMissingFilePreview renders a moved-or-deleted selected file notice.
// WorkspaceMissingFilePreview 渲染已选中文件被移动或删除的提示。
function WorkspaceMissingFilePreview(props: {
  // Previously selected file path that no longer resolves in the tree.
  // 先前选中但已无法在目录树中解析的文件路径。
  missingPath: string;
}) {
  const { t } = useI18n();
  return (
    <div className="workspace-file-preview empty error">
      <FileText size={24} />
      <strong>{t("workspaceFilePreview.missingTitle")}</strong>
      <span>{props.missingPath}</span>
    </div>
  );
}

// WorkspaceMediaPermissionPrompt asks before enabling image or video previews.
// WorkspaceMediaPermissionPrompt 在启用图片或视频预览前请求确认。
function WorkspaceMediaPermissionPrompt(props: {
  // Selected media entry waiting for consent.
  // 等待授权的已选中媒体条目。
  entry: WorkspaceFileEntrySummary;
  // Grants media display permission and opens the selected media file.
  // 授权媒体展示并打开已选中的媒体文件。
  onAllowMediaPreview: (entry: WorkspaceFileEntrySummary) => void;
}) {
  const { t } = useI18n();
  // mediaLabel stores the localized media kind name.
  // mediaLabel 保存本地化媒体类型名称。
  const mediaLabel = props.entry.kind === "video" ? t("workspaceFilePreview.video") : t("workspaceFilePreview.image");
  return (
    <div className="workspace-file-preview empty media-permission">
      <FileText size={24} />
      <strong>{t("workspaceFilePreview.mediaBlockedTitle").replace("{media}", mediaLabel)}</strong>
      <span>{t("workspaceFilePreview.mediaBlockedHint").replace("{media}", mediaLabel)}</span>
      <Button size="sm" type="button" onClick={() => props.onAllowMediaPreview(props.entry)}>
        {t("workspaceFilePreview.allowMedia").replace("{media}", mediaLabel)}
      </Button>
    </div>
  );
}

// WorkspaceMarkdownPreview renders a Markdown file as it reads, with Ctrl+F finding text in it like the code preview.
// WorkspaceMarkdownPreview 按阅读形态渲染 Markdown 文件，并像代码预览一样支持 Ctrl+F 查找其中的文本。
function WorkspaceMarkdownPreview(props: { preview: WorkspaceFileReadResponse }) {
  const bodyRef = React.useRef<HTMLDivElement | null>(null);
  const revealMatch = React.useCallback((range: Range) => {
    range.startContainer.parentElement?.scrollIntoView({ block: "center" });
  }, []);
  return (
    <div className="workspace-file-preview markdown" data-find-scope="">
      <div className="workspace-file-preview-markdown" ref={bodyRef} tabIndex={0}>
        <MarkdownImageWorkspacePathProvider value={props.preview.workspacePath}>
          <MarkdownMessage content={props.preview.content ?? ""} eager />
        </MarkdownImageWorkspacePathProvider>
      </div>
      <ConversationFindBar highlight="vk-file-find" key={props.preview.path} label="workspaceFiles.find" onReveal={revealMatch} scoped scrollRef={bodyRef} />
    </div>
  );
}

// WorkspaceHighlightedCodePreview renders a read-only code preview through the review Shiki pipeline. Selected
// lines can be quoted into the composer under the file's path and line range.
// WorkspaceHighlightedCodePreview 通过审核 Shiki 管线渲染只读代码预览。选中的行可连同文件路径与行范围引用到输入区。
function WorkspaceHighlightedCodePreview(props: {
  // Loaded code preview response.
  // 已加载的代码预览响应。
  preview: WorkspaceFileReadResponse;
}) {
  // diffFile stores a synthetic context-only diff file used to reuse review language and highlight hooks.
  // diffFile 保存合成的纯上下文 diff 文件，用于复用审核语言推断与高亮 hooks。
  const diffFile = React.useMemo(() => workspacePreviewDiffFile(props.preview), [props.preview]);
  // language stores the Shiki language inferred by the review highlighter.
  // language 保存审核高亮器推断出的 Shiki 语言。
  const language = React.useMemo(() => diffLanguageForFile(diffFile), [diffFile]);
  // wrapLines folds long lines into the panel width, as chosen from the file bar.
  // wrapLines 按文件栏中的选择将长行折入面板宽度。
  const [wrapLines] = useWorkspaceFileWrapLines();
  // highlightSupported stores whether the inferred language is handled by the review worker.
  // highlightSupported 保存审核 Worker 是否支持推断语言。
  const highlightSupported = useHighlightLanguage(language);
  // highlightEnabled stores whether this preview is small enough for token highlighting.
  // highlightEnabled 保存当前预览是否足够小以启用 token 高亮。
  const highlightEnabled = Boolean(language && highlightSupported && diffFile.hunks[0].lines.length <= workspaceFilePreviewMaxHighlightRows);
  // highlightRevision requests and tracks Shiki token readiness for the synthetic file.
  // highlightRevision 请求并跟踪合成文件的 Shiki token 就绪状态。
  const highlightRevision = useHighlightedDiffFileRevision(diffFile, language, highlightEnabled);
  // reveal is the request, if any, to show lines of this exact preview; a reloaded preview no longer has one.
  // reveal 是显示本次预览指定行的请求（如有）；重新加载的预览不再带有该请求。
  const reveal = React.useMemo(() => peekWorkspaceFileLineReveal(props.preview.path), [props.preview]);
  // gotoLine is the line the reader went to with Ctrl+G in this exact preview; it replaces the requested lines.
  // gotoLine 是读者在本次预览中通过 Ctrl+G 跳到的行；它取代请求定位的行。
  const [gotoLine, setGotoLine] = React.useState<{ preview: WorkspaceFileReadResponse; line: number } | null>(null);
  const focusLine = gotoLine?.preview === props.preview ? gotoLine.line : null;
  const focus = React.useMemo(
    () => (focusLine ? { start: focusLine, end: focusLine } : reveal?.lines ?? null),
    [focusLine, reveal],
  );
  // rows stores rendered preview line nodes.
  // rows 保存已渲染的预览行节点。
  const rows = React.useMemo(
    () => workspaceHighlightedCodeRows(diffFile.hunks[0].lines, language, highlightRevision, highlightEnabled, focus),
    [diffFile, focus, highlightEnabled, highlightRevision, language],
  );
  const linesRef = React.useRef<HTMLDivElement | null>(null);
  React.useEffect(() => {
    if (!reveal) return;
    consumeWorkspaceFileLineReveal(reveal.id);
    linesRef.current?.querySelector(`[data-line="${reveal.lines.start}"]`)?.scrollIntoView({ block: "center" });
  }, [reveal]);
  // goToLine marks and centres a line, clamped to the file; focus returns to the code afterwards.
  // goToLine 标记并居中某一行（限制在文件范围内）；之后焦点回到代码区。
  const lineCount = diffFile.hunks[0].lines.length;
  const goToLine = React.useCallback((line: number) => {
    const target = Math.min(Math.max(line, 1), lineCount);
    setGotoLine({ line: target, preview: props.preview });
    linesRef.current?.querySelector(`[data-line="${target}"]`)?.scrollIntoView({ block: "center" });
  }, [lineCount, props.preview]);
  // quoteSelection reads the live selection, which the quote action keeps while it is pressed.
  // quoteSelection 读取实时选区；按下引用操作时选区会被保留。
  const quoteSelection = () => {
    const selection = window.getSelection();
    const container = linesRef.current;
    if (!selection || selection.rangeCount === 0 || !container) return;
    const span = previewSelectionLines(container, selection.getRangeAt(0));
    if (!span) return;
    const code = diffFile.hunks[0].lines.slice(span.start - 1, span.end).map((line) => line.content).join("\n");
    requestComposerCommand({ kind: "insert", text: codeQuoteMarkdown(props.preview.relativePath || props.preview.path, span, code, language) });
  };
  // selectFromGutter selects a line when its number is clicked, or the lines from the last clicked number with Shift,
  // so whole lines can be quoted or copied without dragging.
  // selectFromGutter 在点击行号时选中该行，按住 Shift 时选中自上次点击的行号起的各行，无需拖动即可引用或复制整行。
  const gutterAnchor = React.useRef<number | null>(null);
  const selectFromGutter = (event: React.MouseEvent<HTMLDivElement>) => {
    const container = linesRef.current;
    const line = Number((event.target as Element).closest(".workspace-file-preview-code-gutter")?.parentElement?.getAttribute("data-line"));
    if (event.button !== 0 || !line || !container) return;
    event.preventDefault();
    container.focus({ preventScroll: true });
    const anchor = event.shiftKey && gutterAnchor.current && gutterAnchor.current <= lineCount ? gutterAnchor.current : line;
    if (!event.shiftKey) gutterAnchor.current = line;
    selectPreviewLines(container, anchor, line);
  };
  // revealMatch centres a find match vertically and brings it out from under the sticky gutter or past the right edge.
  // revealMatch 将查找匹配垂直居中，并在其被吸附行号栏遮住或超出右缘时横向滚入视野。
  const revealMatch = React.useCallback((range: Range) => {
    const port = linesRef.current;
    if (!port) return;
    const box = range.getBoundingClientRect();
    const view = port.getBoundingClientRect();
    const gutter = port.querySelector(".workspace-file-preview-code-gutter")?.getBoundingClientRect().width ?? 0;
    port.scrollTop += box.top + box.height / 2 - (view.top + view.height / 2);
    if (box.left < view.left + gutter || box.right > view.right) port.scrollLeft += box.left - view.left - gutter - view.width / 4;
  }, []);
  return (
    <>
      {/* Focusable so a click in the code gives Ctrl+F to this file's find bar. 可获得焦点，使点击代码后 Ctrl+F 交给本文件的查找栏。 */}
      <div className="workspace-file-preview-code-lines" data-wrap-lines={wrapLines ? "" : undefined} onMouseDown={selectFromGutter} ref={linesRef} tabIndex={0}>{rows}</div>
      <ConversationSelectionQuote onQuote={quoteSelection} scrollRef={linesRef} timelineRef={linesRef} />
      <ConversationFindBar highlight="vk-file-find" key={props.preview.path} label="workspaceFiles.find" onGotoLine={goToLine} onReveal={revealMatch} scoped scrollRef={linesRef} />
    </>
  );
}

// workspacePreviewDiffFile builds a synthetic context-only diff file for code preview highlighting.
// workspacePreviewDiffFile 为代码预览高亮构建合成的纯上下文 diff 文件。
function workspacePreviewDiffFile(
  // Loaded code preview response.
  // 已加载的代码预览响应。
  preview: WorkspaceFileReadResponse,
): DiffFile {
  const lines = workspacePreviewDiffLines(preview.content ?? "");
  const hunk = {
    header: `@@ -1,${lines.length} +1,${lines.length} @@`,
    oldStart: 1,
    newStart: 1,
    lines,
  };
  return {
    key: preview.path,
    oldPath: preview.relativePath || preview.path,
    newPath: preview.relativePath || preview.path,
    status: "modified",
    additions: 0,
    deletions: 0,
    isBinary: false,
    isGitlink: false,
    changedLines: 0,
    changedBytes: preview.sizeBytes,
    maxChangedLineBytes: 0,
    isLarge: false,
    largeReasons: [],
    hunks: [hunk],
  };
}

// workspacePreviewDiffLines converts plain source content into context rows with line numbers.
// workspacePreviewDiffLines 将纯源码内容转换为带行号的上下文行。
function workspacePreviewDiffLines(
  // Source text returned by the workspace file preview RPC.
  // 工作区文件预览 RPC 返回的源码文本。
  content: string,
): DiffLine[] {
  const sourceLines = content.split(/\r?\n/);
  if (sourceLines.length > 1 && sourceLines[sourceLines.length - 1] === "") {
    sourceLines.pop();
  }
  return sourceLines.map((line, index) => ({
    kind: "context",
    content: line,
    oldLine: index + 1,
    newLine: index + 1,
  }));
}

// workspaceHighlightedCodeRows renders numbered source rows with cached Shiki token HTML when available.
// workspaceHighlightedCodeRows 使用可用的缓存 Shiki token HTML 渲染带行号源码行。
function workspaceHighlightedCodeRows(
  // Synthetic diff lines representing the preview source.
  // 表示预览源码的合成 diff 行。
  lines: DiffLine[],
  // Inferred Shiki language.
  // 推断出的 Shiki 语言。
  language: string | null,
  // Highlight revision used to refresh memoized rows when worker results arrive.
  // Worker 结果到达时刷新记忆化行使用的高亮修订号。
  _highlightRevision: number,
  // Whether highlighted rendering should be attempted.
  // 是否应尝试高亮渲染。
  highlightEnabled: boolean,
  // Lines a file open request asked to show, marked on their rows.
  // 开文件请求要求显示的行，在对应行上标记。
  focus: WorkspaceFileLines | null,
): React.ReactNode[] {
  const rows: React.ReactNode[] = new Array(lines.length);
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const highlightedSpans = highlightEnabled && language
      ? highlightedDiffLineSpansForDiffLine(line, language)
      : null;
    const html = highlightedSpans ? tokenSpansToHtml(highlightedSpans) : escapeHtml(line.content);
    rows[index] = (
      <div className="workspace-file-preview-code-row" data-focus={focus && index + 1 >= focus.start && index + 1 <= focus.end ? "" : undefined} data-line={index + 1} key={index}>
        <span className="workspace-file-preview-code-gutter" data-find-skip="">{index + 1}</span>
        <span
          className="workspace-file-preview-code-content"
          dangerouslySetInnerHTML={{ __html: html || " " }}
        />
      </div>
    );
  }
  return rows;
}

// workspaceFilePreviewErrorMessage converts backend preview failures into user-facing text.
// workspaceFilePreviewErrorMessage 将后端预览失败转换为面向用户的提示文本。
export function workspaceFilePreviewErrorMessage(
  // Preview load error from the service edge.
  // 来自服务边界的预览加载错误。
  error: unknown,
  // File entry that was being previewed.
  // 正在预览的文件条目。
  entry: WorkspaceFileEntrySummary,
  // Translation helper for user-facing preview errors.
  // 用户可见预览错误的翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  // message stores the raw backend or runtime error text.
  // message 保存后端或运行时返回的原始错误文本。
  const message = error instanceof Error ? error.message : String(error);
  // normalizedMessage stores a lowercase copy used for portable missing-file detection.
  // normalizedMessage 保存小写副本，用于跨平台判断文件缺失。
  const normalizedMessage = message.toLowerCase();
  if (
    message.includes("Failed to read file metadata")
    || message.includes("系统找不到指定的文件")
    || normalizedMessage.includes("no such file")
    || normalizedMessage.includes("not found")
    || normalizedMessage.includes("cannot find")
  ) {
    return t("workspaceFilePreview.missingError").replace("{path}", entry.relativePath || entry.path);
  }
  return message;
}
