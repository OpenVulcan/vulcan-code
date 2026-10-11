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
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { workspaceFileTreeExtensionLabel } from "@/components/workspace/workspace-file-tree-drawer";
import { useI18n, type TranslationKey } from "@/i18n";
import type { WorkspaceFileEntrySummary, WorkspaceFileReadResponse } from "@/types";

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
  // Closes the current preview and opens the directory drawer.
  // 关闭当前预览并打开目录抽屉。
  onClosePreview: () => void;
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
    return <div className="workspace-file-preview"><StreamPlaceholder className="p-4" label={t("workspaceFilePreview.loading")} count={8} /></div>;
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
        <WorkspaceFilePreviewHeader preview={props.preview} onClosePreview={props.onClosePreview} />
        <img alt={props.preview.relativePath} decoding="async" src={props.resolveMediaSourceUrl(props.preview.path)} />
      </div>
    );
  }
  if (props.preview.kind === "video") {
    return (
      <div className="workspace-file-preview media">
        <WorkspaceFilePreviewHeader preview={props.preview} onClosePreview={props.onClosePreview} />
        <video aria-label={props.preview.relativePath} controls preload="metadata" src={props.resolveMediaSourceUrl(props.preview.path)} />
      </div>
    );
  }
  if (props.entry.extension === "md") {
    return (
      <div className="workspace-file-preview markdown">
        <WorkspaceFilePreviewHeader preview={props.preview} onClosePreview={props.onClosePreview} />
        <div className="workspace-file-preview-markdown">
          <MarkdownImageWorkspacePathProvider value={props.preview.workspacePath}>
            <MarkdownMessage content={props.preview.content ?? ""} eager />
          </MarkdownImageWorkspacePathProvider>
        </div>
      </div>
    );
  }
  return (
    <div className="workspace-file-preview code">
      <WorkspaceFilePreviewHeader preview={props.preview} onClosePreview={props.onClosePreview} />
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

// WorkspaceFilePreviewHeader renders the glass overlay title used to reopen the directory drawer.
// WorkspaceFilePreviewHeader 渲染用于重新打开目录抽屉的玻璃覆盖标题。
// Parameters: props identifies the preview title/path and directory action; returns the named control for reopening its directory.
// 参数：props 标识预览标题和路径及目录操作；返回具有名称的重新打开目录控件。
function WorkspaceFilePreviewHeader(props: {
  // Loaded preview response shown by the header.
  // 标题展示的已加载预览响应。
  preview: WorkspaceFileReadResponse;
  // Closes the current preview and opens the directory drawer.
  // 关闭当前预览并打开目录抽屉。
  onClosePreview: () => void;
}) {
  const { t } = useI18n();
  // iconLabel stores the same compact file-type label used by the shared tree row.
  // iconLabel 保存与共享目录树行一致的紧凑文件类型标签。
  const iconLabel = workspaceFilePreviewIconLabel(props.preview);
  // iconClassName stores the same file-kind palette used by the shared tree row.
  // iconClassName 保存与共享目录树行一致的文件类型色板。
  const iconClassName = workspaceFilePreviewIconClassName(props.preview);
  return (
    <button className="workspace-file-preview-header" onClick={props.onClosePreview} type="button"
      aria-label={t("workspaceAudit.openDirectory").replace("{path}", props.preview.relativePath)}>
      <span className={`diff-file-tree-file-icon workspace-file-preview-format-icon ${iconClassName}`}>
        {iconLabel}
      </span>
      <span className="workspace-file-preview-title-text">{props.preview.relativePath}</span>
      {props.preview.truncated ? <small>{t("workspaceFilePreview.truncated")}</small> : null}
    </button>
  );
}

// WorkspaceHighlightedCodePreview renders a read-only code preview through the review Shiki pipeline.
// WorkspaceHighlightedCodePreview 通过审核 Shiki 管线渲染只读代码预览。
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
  // highlightSupported stores whether the inferred language is handled by the review worker.
  // highlightSupported 保存审核 Worker 是否支持推断语言。
  const highlightSupported = useHighlightLanguage(language);
  // highlightEnabled stores whether this preview is small enough for token highlighting.
  // highlightEnabled 保存当前预览是否足够小以启用 token 高亮。
  const highlightEnabled = Boolean(language && highlightSupported && diffFile.hunks[0].lines.length <= workspaceFilePreviewMaxHighlightRows);
  // highlightRevision requests and tracks Shiki token readiness for the synthetic file.
  // highlightRevision 请求并跟踪合成文件的 Shiki token 就绪状态。
  const highlightRevision = useHighlightedDiffFileRevision(diffFile, language, highlightEnabled);
  // rows stores rendered preview line nodes.
  // rows 保存已渲染的预览行节点。
  const rows = React.useMemo(
    () => workspaceHighlightedCodeRows(diffFile.hunks[0].lines, language, highlightRevision, highlightEnabled),
    [diffFile, highlightEnabled, highlightRevision, language],
  );
  return <div className="workspace-file-preview-code-lines">{rows}</div>;
}

// workspaceFilePreviewIconLabel returns the compact file-type label for the preview title.
// workspaceFilePreviewIconLabel 返回预览标题使用的紧凑文件类型标签。
function workspaceFilePreviewIconLabel(
  // Loaded preview response shown by the title.
  // 标题展示的已加载预览响应。
  preview: WorkspaceFileReadResponse,
): string {
  if (preview.kind === "image") {
    return "IMG";
  }
  if (preview.kind === "video") {
    return "VID";
  }
  return workspaceFileTreeExtensionLabel(preview.relativePath || preview.path);
}

// workspaceFilePreviewIconClassName maps preview kinds into the shared review icon palette.
// workspaceFilePreviewIconClassName 将预览类型映射到共享审核图标色板。
function workspaceFilePreviewIconClassName(
  // Loaded preview response shown by the title.
  // 标题展示的已加载预览响应。
  preview: WorkspaceFileReadResponse,
): string {
  return preview.kind === "code" ? "modified" : "";
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
): React.ReactNode[] {
  const rows: React.ReactNode[] = new Array(lines.length);
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const highlightedSpans = highlightEnabled && language
      ? highlightedDiffLineSpansForDiffLine(line, language)
      : null;
    const html = highlightedSpans ? tokenSpansToHtml(highlightedSpans) : escapeHtml(line.content);
    rows[index] = (
      <div className="workspace-file-preview-code-row" key={index}>
        <span className="workspace-file-preview-code-gutter">{index + 1}</span>
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
