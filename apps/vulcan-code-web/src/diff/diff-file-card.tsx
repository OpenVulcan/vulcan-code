import React from "react";
import { AlertTriangle, ChevronDown, ChevronRight } from "lucide-react";

import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n-messages";
import type { DiffFile, DiffHunkExpansionDirection, DiffHunkExpansionRequest, DiffRenderPreferences, DiffViewMode, WorkspaceDiffSearchFileMatch } from "./types";
import {
  diffLanguageForFile,
  useHighlightedDiffFileRevision,
  useHighlightedDiffWindowRevision,
  useHighlightLanguage,
} from "./diff-highlight";
import {
  DIFF_FILE_CHANGED_BYTES_LIMIT,
  DIFF_FILE_CHANGED_LINES_LIMIT,
  DIFF_FILE_MAX_LINE_BYTES_LIMIT,
  DIFF_LIGHTWEIGHT_DETAIL_CHANGED_LINES_LIMIT,
  DIFF_FILE_VIRTUALIZATION_THRESHOLD,
  DIFF_HUNK_HEADER_HEIGHT,
  DIFF_LINE_HEIGHT,
  DIFF_SCROLL_STATE_GRANULARITY_PX,
  DIFF_SHIKI_TOKEN_ROW_LIMIT,
  DIFF_TOTAL_CHANGED_BYTES_LIMIT,
  DIFF_TOTAL_CHANGED_LINES_LIMIT,
  DIFF_TOTAL_FILES_LIMIT,
  shouldUseLightweightDiffRendering,
} from "./limits";
import {
  type DiffMountedVirtualRow,
  type DiffScrollState,
  buildDiffVirtualLayout,
  elementTopWithinScrollContainer,
  mountedDiffRows,
} from "./diff-virtual-layout";
import {
  useHighlightedDiffFileHtml,
  useHighlightedDiffRowsHtml,
  usePlainDiffFileHtml,
  usePlainDiffRowsHtml,
} from "./diff-render-worker-client";
import { setupTargetDiffDom } from "./target-diff-dom";
import { targetDiffFileVisualRowCount } from "./target-diff-model";
import { DiffFileLoadStateNote } from "./diff-file-card/load-state";
import { formatBytes } from "@/lib/display-format";

// DIFF_DEFAULT_CONTEXT_LINES matches Git's default unified-diff context.
// DIFF_DEFAULT_CONTEXT_LINES 对齐 Git 默认 unified-diff 上下文行数。
const DIFF_DEFAULT_CONTEXT_LINES = 3;

// DIFF_CONTEXT_EXPANSION_INCREMENT defines the per-click hunk context growth.
// DIFF_CONTEXT_EXPANSION_INCREMENT 定义每次点击时 hunk 上下文增长的行数。
const DIFF_CONTEXT_EXPANSION_INCREMENT = 50;

// diffFileLoadedRowsHeightCache stores full loaded-file body heights per stable diff file.
// diffFileLoadedRowsHeightCache 按稳定 diff 文件缓存完整已加载文件体高度。
const diffFileLoadedRowsHeightCache = new WeakMap<DiffFile, number>();

// diffMountedRowsHeightCache stores mounted-window heights per stable row array.
// diffMountedRowsHeightCache 按稳定行数组缓存挂载窗口高度。
const diffMountedRowsHeightCache = new WeakMap<readonly DiffMountedVirtualRow[], number>();

// diffFileLoadedRowsPlaceholderStyleCache stores stable pending placeholder styles per loaded file.
// diffFileLoadedRowsPlaceholderStyleCache 按已加载文件缓存稳定等待占位样式。
const diffFileLoadedRowsPlaceholderStyleCache = new WeakMap<DiffFile, React.CSSProperties>();

// diffMountedRowsPlaceholderStyleCache stores stable pending placeholder styles per mounted row window.
// diffMountedRowsPlaceholderStyleCache 按挂载行窗口缓存稳定等待占位样式。
const diffMountedRowsPlaceholderStyleCache = new WeakMap<readonly DiffMountedVirtualRow[], React.CSSProperties>();

// EMPTY_DIFF_RENDER_SEARCH_LINE_KEYS keeps worker-rendered HTML independent from live search input.
// EMPTY_DIFF_RENDER_SEARCH_LINE_KEYS 让 Worker 渲染 HTML 与实时搜索输入解耦。
const EMPTY_DIFF_RENDER_SEARCH_LINE_KEYS: ReadonlySet<string> = new Set<string>();

// diffRowsDomSetupKey captures mode preferences that require rebinding target DOM managers.
// diffRowsDomSetupKey 捕获需要重新绑定目标 DOM 管理器的模式偏好。
function diffRowsDomSetupKey(
  // Current visual diff mode.
  // 当前 diff 视觉模式。
  viewMode: DiffViewMode,
  // Current target-compatible render preferences.
  // 当前目标兼容渲染偏好。
  preferences: DiffRenderPreferences,
): string {
  return `${viewMode}:${preferences.overflowMode}:${preferences.hunkSeparatorStyle}:${preferences.indicatorStyle}`;
}

// DiffFileCardProps stores render inputs for one memoized file card.
// DiffFileCardProps 保存单个记忆化文件卡片的渲染输入。
interface DiffFileCardProps {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Whether this file is currently collapsed by the review panel.
  // 该文件当前是否被审核面板折叠。
  collapsed: boolean;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
  // Estimated line host top supplied by file-list virtualization.
  // 文件列表虚拟化提供的行宿主估算顶部。
  virtualHostTop?: number;
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode: DiffViewMode;
  // Target-compatible rendering preferences.
  // 目标兼容的渲染偏好。
  renderPreferences: DiffRenderPreferences;
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行键。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行键。
  activeSearchLineKey?: string | null;
  // Optional lazy detail loader used by retryable summary-only files.
  // 可选的懒加载详情函数，用于可重试的摘要文件。
  onLoadFileDetail?: (file: DiffFile, contextLines?: number, expansion?: DiffHunkExpansionRequest) => void;
  // Optional host action that reveals this file in the system file manager.
  // 可选的宿主动作，用于在系统文件管理器中定位该文件。
  onRevealFile?: (file: DiffFile) => void;
  // Leaves out the file-name header when the surrounding panel already names the file.
  // 外层面板已标明文件时省略文件名头部。
  headerless?: boolean;
  // Optional backend search hit shown before full detail hydration finishes.
  // 完整详情水合完成前展示的可选后端搜索命中。
  summarySearchMatch?: WorkspaceDiffSearchFileMatch;
  // Records a user-triggered file collapse state change.
  // 记录用户触发的文件折叠状态变化。
  onCollapsedChange: (fileKey: string, collapsed: boolean) => void;
}

// DiffFileHeader renders the reusable file-name header for normal and sticky layers.
// DiffFileHeader 渲染普通层与吸顶层复用的文件名头部。
export function DiffFileHeader(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Whether this file is currently collapsed.
  // 该文件当前是否处于折叠状态。
  collapsed: boolean;
  // Optional extra class name for sticky overlay styling.
  // 用于吸顶覆盖层样式的可选额外类名。
  className?: string;
  // Records a user-triggered collapse toggle.
  // 记录用户触发的折叠切换。
  onToggle: () => void;
}) {
  const { t } = useI18n();
  const ToggleIcon = props.collapsed ? ChevronRight : ChevronDown;
  return (
    <button aria-expanded={!props.collapsed} className={props.className ? `diff-file-header ${props.className}` : "diff-file-header"} onClick={props.onToggle} type="button">
      <ToggleIcon size={15} />
      <div className="diff-file-paths">
        <strong>
          {props.file.tooLargeToDisplay ? <AlertTriangle aria-label={t("diff.file.tooLarge")} className="diff-file-large-icon" size={13} /> : null}
          <span>{displayPath(props.file)}</span>
          {props.file.status === "added" ? <span aria-label={t("diff.file.added")} className="diff-file-status-dot added" /> : null}
          {props.file.status === "deleted" ? <span aria-label={t("diff.file.deleted")} className="diff-file-status-dot deleted" /> : null}
        </strong>
        {props.file.status === "renamed" ? <span>{`${props.file.oldPath} -> ${props.file.newPath}`}</span> : null}
      </div>
      <div className="diff-file-meta">
        <span className="diff-stat added">+{props.file.additions}</span>
        <span className="diff-stat removed">-{props.file.deletions}</span>
      </div>
    </button>
  );
}

// DiffFileCard renders one file-level diff card.
// DiffFileCard 渲染单个文件级 diff 卡片。
export const DiffFileCard = React.memo(function DiffFileCard(props: DiffFileCardProps) {
  const { t } = useI18n();
  const rowCount = React.useMemo(() => targetDiffFileVisualRowCount(props.file, props.viewMode), [props.file, props.viewMode]);
  const lightweightRendering = shouldUseLightweightDiffRendering(props.file);
  const shouldUseShikiTokens = !lightweightRendering && rowCount <= DIFF_SHIKI_TOKEN_ROW_LIMIT;
  const inlineDiff = props.renderPreferences.inlineDiff;
  const language = !props.collapsed && shouldUseShikiTokens ? diffLanguageForFile(props.file) : null;
  const highlightSupported = useHighlightLanguage(language);
  const contextExpansionAvailable = props.onLoadFileDetail && !props.file.tooLargeToDisplay ? canExpandDiffFileContext(props.file) : false;
  const handleToggle = React.useCallback(() => {
    props.onCollapsedChange(props.file.key, !props.collapsed);
  }, [props.collapsed, props.file.key, props.onCollapsedChange]);
  const handleExpandFileContext = React.useCallback((hunkIndex: number, direction: DiffHunkExpansionDirection) => {
    if (canExpandDiffFileContext(props.file)) {
      props.onLoadFileDetail?.(props.file, nextExpandedContextLineCount(props.file), { hunkIndex, direction });
    }
  }, [props.file, props.onLoadFileDetail]);
  return (
    <article className={`diff-file-card ${props.file.status}`}>
      {props.headerless ? null : <DiffFileHeader collapsed={props.collapsed} file={props.file} onToggle={handleToggle} />}
      {!props.collapsed ? (
        <div className="diff-file-body">
          {props.file.isBinary ? (
            <div className="diff-binary-note">{t("diff.file.binary")}</div>
          ) : props.file.isGitlink ? (
            <div className="diff-binary-note">{t("diff.file.gitlink")}</div>
          ) : props.file.hunks.length ? (
            <>
              <DiffRenderPolicyNote file={props.file} lightweightRendering={lightweightRendering} rowCount={rowCount} />
              <DiffFileLines
                file={props.file}
                highlightSupported={highlightSupported}
                language={language}
                rowCount={rowCount}
                inlineDiff={inlineDiff}
                viewMode={props.viewMode}
                renderPreferences={props.renderPreferences}
                scrollContainerRef={props.scrollContainerRef}
                scrollState={props.scrollState}
                virtualHostTop={props.virtualHostTop}
                activeSearchLineKey={props.activeSearchLineKey}
                searchLineKeys={props.searchLineKeys}
                onExpandFileContext={contextExpansionAvailable ? handleExpandFileContext : undefined}
              />
            </>
          ) : props.file.isPartial ? (
            <DiffFileLoadStateNote
              file={props.file}
              summarySearchMatch={props.summarySearchMatch}
              onRetry={props.onLoadFileDetail}
              onRevealFile={props.onRevealFile}
            />
          ) : props.file.status === "renamed" ? (
            <div className="diff-binary-note">{t("diff.file.renamedNoChanges")}</div>
          ) : (
            <div className="diff-binary-note">{t("diff.file.noContent")}</div>
          )}
        </div>
      ) : null}
    </article>
  );
}, areDiffFileCardPropsEqual);

// DiffRenderPolicyNote explains the lightweight rendering policy inside the new renderer.
// DiffRenderPolicyNote 在新渲染器内部解释轻量渲染策略。
function DiffRenderPolicyNote(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Whether expensive detail rendering is skipped by the lightweight policy.
  // 昂贵细节渲染是否被轻量策略省略。
  lightweightRendering: boolean;
  // Precomputed visual row count for this file.
  // 该文件预先计算的可视行数。
  rowCount: number;
}) {
  const { t } = useI18n();
  if (!props.lightweightRendering) {
    return null;
  }
  const reasonText = props.file.largeReasons.length
    ? formatLargeReasons(props.file.largeReasons, t)
    : props.file.changedLines > DIFF_LIGHTWEIGHT_DETAIL_CHANGED_LINES_LIMIT
      ? formatDiffCardTranslation(t, "diff.render.changedLinesSkipped", { limit: String(DIFF_LIGHTWEIGHT_DETAIL_CHANGED_LINES_LIMIT) })
      : t("diff.render.skipped");
  return (
    <div className="diff-render-policy-note">
      <span>{reasonText}</span>
      <span>{formatDiffCardTranslation(t, "diff.render.rowsLightweight", { count: String(props.rowCount) })}</span>
    </div>
  );
}

// formatLargeReasons converts stable backend reason ids into localized display labels.
// formatLargeReasons 将稳定后端原因标识转换为本地化展示文案。
function formatLargeReasons(reasons: readonly string[], t: (key: TranslationKey) => string): string {
  return reasons.map((reason) => largeReasonLabel(reason, t)).join(t("global.listDelimiter"));
}

// largeReasonLabel maps one known large-diff reason id to its localized label.
// largeReasonLabel 将单个已知大 diff 原因标识映射为本地化标签。
function largeReasonLabel(reason: string, t: (key: TranslationKey) => string): string {
  if (reason === "total_files") {
    return formatDiffCardTranslation(t, "diff.large.totalFiles", { limit: String(DIFF_TOTAL_FILES_LIMIT) });
  }
  if (reason === "total_changed_lines") {
    return formatDiffCardTranslation(t, "diff.large.totalChangedLines", { limit: String(DIFF_TOTAL_CHANGED_LINES_LIMIT) });
  }
  if (reason === "total_changed_bytes") {
    return formatDiffCardTranslation(t, "diff.large.totalChangedBytes", { limit: formatBytes(DIFF_TOTAL_CHANGED_BYTES_LIMIT) });
  }
  if (reason === "file_changed_lines") {
    return formatDiffCardTranslation(t, "diff.large.fileChangedLines", { limit: String(DIFF_FILE_CHANGED_LINES_LIMIT) });
  }
  if (reason === "file_changed_bytes") {
    return formatDiffCardTranslation(t, "diff.large.fileChangedBytes", { limit: formatBytes(DIFF_FILE_CHANGED_BYTES_LIMIT) });
  }
  if (reason === "file_max_line_bytes") {
    return formatDiffCardTranslation(t, "diff.large.fileMaxLineBytes", { limit: formatBytes(DIFF_FILE_MAX_LINE_BYTES_LIMIT) });
  }
  return reason;
}

// formatDiffCardTranslation applies small named placeholders for diff card labels.
// formatDiffCardTranslation 为 diff 文件卡片标签替换少量具名占位符。
function formatDiffCardTranslation(t: (key: TranslationKey) => string, key: TranslationKey, values: Record<string, string>): string {
  let text = t(key);
  for (const [name, value] of Object.entries(values)) {
    text = text.replace(`{${name}}`, value);
  }
  return text;
}

// DiffFileLines switches between direct and virtualized rendering for one file.
// DiffFileLines 为单个文件在直接渲染和虚拟渲染之间切换。
function DiffFileLines(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the Shiki worker supports the inferred language.
  // Shiki Worker 是否支持推断出的语言。
  highlightSupported: boolean;
  // Precomputed visual row count for this file.
  // 该文件预先计算的可视行数。
  rowCount: number;
  // Whether paired changed lines should render inline change markers.
  // 配对变更行是否应渲染行内变化标记。
  inlineDiff: boolean;
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode: DiffViewMode;
  // Target-compatible rendering preferences.
  // 目标兼容的渲染偏好。
  renderPreferences: DiffRenderPreferences;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
  // Estimated line host top supplied by file-list virtualization.
  // 文件列表虚拟化提供的行宿主估算顶部。
  virtualHostTop?: number;
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行键。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行键。
  activeSearchLineKey?: string | null;
  // Triggered when a hunk header asks for a larger Git context window.
  // hunk 头部请求更大的 Git 上下文窗口时触发。
  onExpandFileContext?: (hunkIndex: number, direction: DiffHunkExpansionDirection) => void;
}) {
  if (props.rowCount < DIFF_FILE_VIRTUALIZATION_THRESHOLD) {
    return (
      <DirectDiffFileLines
        file={props.file}
        highlightSupported={props.highlightSupported}
        language={props.language}
        inlineDiff={props.inlineDiff}
        viewMode={props.viewMode}
        renderPreferences={props.renderPreferences}
        activeSearchLineKey={props.activeSearchLineKey}
        searchLineKeys={props.searchLineKeys}
        onExpandFileContext={props.onExpandFileContext}
      />
    );
  }
  return (
    <VirtualDiffFileLines
      file={props.file}
      highlightSupported={props.highlightSupported}
      language={props.language}
      inlineDiff={props.inlineDiff}
      viewMode={props.viewMode}
      renderPreferences={props.renderPreferences}
      scrollContainerRef={props.scrollContainerRef}
      scrollState={props.scrollState}
      virtualHostTop={props.virtualHostTop}
      activeSearchLineKey={props.activeSearchLineKey}
      searchLineKeys={props.searchLineKeys}
      onExpandFileContext={props.onExpandFileContext}
    />
  );
}

// DirectDiffFileLines renders one small file as a cached HTML block.
// DirectDiffFileLines 将一个小文件渲染为缓存后的 HTML 块。
function DirectDiffFileLines(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the Shiki worker supports the inferred language.
  // Shiki Worker 是否支持推断出的语言。
  highlightSupported: boolean;
  // Whether paired changed lines should render inline change markers.
  // 配对变更行是否应渲染行内变化标记。
  inlineDiff: boolean;
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode: DiffViewMode;
  // Target-compatible rendering preferences.
  // 目标兼容的渲染偏好。
  renderPreferences: DiffRenderPreferences;
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行键。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行键。
  activeSearchLineKey?: string | null;
  // Triggered when a hunk header asks for a larger Git context window.
  // hunk 头部请求更大的 Git 上下文窗口时触发。
  onExpandFileContext?: (hunkIndex: number, direction: DiffHunkExpansionDirection) => void;
}) {
  const useHighlightedRenderer = Boolean(props.language && props.highlightSupported);
  const highlightRevision = useHighlightedDiffFileRevision(props.file, props.language, useHighlightedRenderer);
  const plainHtml = usePlainDiffFileHtml({
    file: props.file,
    searchLineKeys: EMPTY_DIFF_RENDER_SEARCH_LINE_KEYS,
    activeSearchLineKey: null,
    contextControls: Boolean(props.onExpandFileContext),
    hunkSeparatorStyle: props.renderPreferences.hunkSeparatorStyle,
    indicatorStyle: props.renderPreferences.indicatorStyle,
    inlineDiff: props.inlineDiff,
    overflowMode: props.renderPreferences.overflowMode,
    viewMode: props.viewMode,
  }, !useHighlightedRenderer);
  const highlightedHtml = useHighlightedDiffFileHtml({
    file: props.file,
    language: props.language ?? "text",
    highlightRevision,
    searchLineKeys: EMPTY_DIFF_RENDER_SEARCH_LINE_KEYS,
    activeSearchLineKey: null,
    contextControls: Boolean(props.onExpandFileContext),
    hunkSeparatorStyle: props.renderPreferences.hunkSeparatorStyle,
    indicatorStyle: props.renderPreferences.indicatorStyle,
    inlineDiff: props.inlineDiff,
    overflowMode: props.renderPreferences.overflowMode,
    viewMode: props.viewMode,
  }, useHighlightedRenderer);
  const html = useHighlightedRenderer ? highlightedHtml : plainHtml;
  return (
    <DiffRowsHtml
      activeSearchLineKey={props.activeSearchLineKey}
      domSetupKey={diffRowsDomSetupKey(props.viewMode, props.renderPreferences)}
      html={html}
      key={diffRowsDomSetupKey(props.viewMode, props.renderPreferences)}
      onExpandFileContext={props.onExpandFileContext}
      pendingStyle={diffFileLoadedRowsPlaceholderStyle(props.file)}
      searchLineKeys={props.searchLineKeys}
    />
  );
}

// areDiffFileCardPropsEqual avoids re-rendering unaffected file cards during scroll or collapse.
// areDiffFileCardPropsEqual 避免滚动或折叠时重渲染未受影响的文件卡片。
function areDiffFileCardPropsEqual(previous: DiffFileCardProps, next: DiffFileCardProps): boolean {
  if (
    previous.file !== next.file
        || previous.scrollContainerRef !== next.scrollContainerRef
    || previous.viewMode !== next.viewMode
    || previous.renderPreferences !== next.renderPreferences
    || previous.virtualHostTop !== next.virtualHostTop
    || previous.collapsed !== next.collapsed
    || previous.headerless !== next.headerless
    || previous.searchLineKeys !== next.searchLineKeys
    || previous.activeSearchLineKey !== next.activeSearchLineKey
    || previous.onLoadFileDetail !== next.onLoadFileDetail
    || previous.onRevealFile !== next.onRevealFile
    || previous.summarySearchMatch !== next.summarySearchMatch
    || previous.onCollapsedChange !== next.onCollapsedChange
  ) {
    return false;
  }
  if (targetDiffFileVisualRowCount(next.file, next.viewMode) < DIFF_FILE_VIRTUALIZATION_THRESHOLD) {
    return true;
  }
  return previous.scrollState.scrollTop === next.scrollState.scrollTop
    && previous.scrollState.clientHeight === next.scrollState.clientHeight;
}

// VirtualDiffFileLines renders only the visible rows for one large file diff.
// VirtualDiffFileLines 仅渲染大文件 diff 当前可见的行。
function VirtualDiffFileLines(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the Shiki worker supports the inferred language.
  // Shiki Worker 是否支持推断出的语言。
  highlightSupported: boolean;
  // Whether paired changed lines should render inline change markers.
  // 配对变更行是否应渲染行内变化标记。
  inlineDiff: boolean;
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode: DiffViewMode;
  // Target-compatible rendering preferences.
  // 目标兼容的渲染偏好。
  renderPreferences: DiffRenderPreferences;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
  // Estimated line host top supplied by file-list virtualization.
  // 文件列表虚拟化提供的行宿主估算顶部。
  virtualHostTop?: number;
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行键。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行键。
  activeSearchLineKey?: string | null;
  // Triggered when a hunk header asks for a larger Git context window.
  // hunk 头部请求更大的 Git 上下文窗口时触发。
  onExpandFileContext?: (hunkIndex: number, direction: DiffHunkExpansionDirection) => void;
}) {
  const hostRef = React.useRef<HTMLDivElement | null>(null);
  const [hostTop, setHostTop] = React.useState(0);
  const layout = React.useMemo(() => buildDiffVirtualLayout(props.file, props.viewMode), [props.file, props.viewMode]);
  const effectiveHostTop = props.virtualHostTop ?? hostTop;
  // measureRevision only follows scroll changes when DOM measurement is still required.
  // measureRevision 仅在仍需 DOM 测量时跟随滚动变化。
  const measureRevision = props.virtualHostTop == null ? props.scrollState.revision : 0;
  // window contains the stable visible row window from the virtual layout cache.
  // window 保存来自虚拟布局缓存的稳定可见行窗口。
  const window = React.useMemo(
    () => mountedDiffRows(props.file, layout, props.scrollState.scrollTop - effectiveHostTop, props.scrollState.clientHeight),
    [effectiveHostTop, layout, props.file, props.scrollState.clientHeight, props.scrollState.scrollTop],
  );
  const useHighlightedRenderer = Boolean(props.language && props.highlightSupported);
  const highlightRevision = useHighlightedDiffWindowRevision(window.rows, props.language, useHighlightedRenderer);
  const plainRowsHtml = usePlainDiffRowsHtml({
    fileKey: props.file.key,
    rows: window.rows,
    searchLineKeys: EMPTY_DIFF_RENDER_SEARCH_LINE_KEYS,
    activeSearchLineKey: null,
    contextControls: Boolean(props.onExpandFileContext),
    hunkSeparatorStyle: props.renderPreferences.hunkSeparatorStyle,
    indicatorStyle: props.renderPreferences.indicatorStyle,
    inlineDiff: props.inlineDiff,
    overflowMode: props.renderPreferences.overflowMode,
    viewMode: props.viewMode,
  }, !useHighlightedRenderer);
  const highlightedRowsHtml = useHighlightedDiffRowsHtml({
    fileKey: props.file.key,
    rows: window.rows,
    language: props.language ?? "text",
    highlightRevision,
    searchLineKeys: EMPTY_DIFF_RENDER_SEARCH_LINE_KEYS,
    activeSearchLineKey: null,
    contextControls: Boolean(props.onExpandFileContext),
    hunkSeparatorStyle: props.renderPreferences.hunkSeparatorStyle,
    indicatorStyle: props.renderPreferences.indicatorStyle,
    inlineDiff: props.inlineDiff,
    overflowMode: props.renderPreferences.overflowMode,
    viewMode: props.viewMode,
  }, useHighlightedRenderer);
  const rowsHtml = useHighlightedRenderer ? highlightedRowsHtml : plainRowsHtml;

  React.useLayoutEffect(() => {
    if (props.virtualHostTop != null) {
      return undefined;
    }
    const host = hostRef.current;
    const container = props.scrollContainerRef.current;
    if (!host || !container) {
      return undefined;
    }
    // A single layout-effect measurement keeps row virtualization aligned without adding a second scroll-time layout read.
    // 单次 layout-effect 测量即可保持行虚拟化对齐，避免滚动时额外增加第二次布局读取。
    const nextTop = elementTopWithinScrollContainer(host, container);
    setHostTop((previousTop) => (Math.abs(previousTop - nextTop) > 0.5 ? nextTop : previousTop));
    return undefined;
  }, [layout.totalHeight, measureRevision, props.scrollContainerRef, props.virtualHostTop]);

  return (
    <div className="diff-virtual-lines" ref={hostRef}>
      {window.paddingTop > 0 ? <div aria-hidden="true" className="diff-virtual-spacer" style={window.topSpacerStyle} /> : null}
      <DiffRowsHtml
        activeSearchLineKey={props.activeSearchLineKey}
        domSetupKey={diffRowsDomSetupKey(props.viewMode, props.renderPreferences)}
        html={rowsHtml}
        key={diffRowsDomSetupKey(props.viewMode, props.renderPreferences)}
        onExpandFileContext={props.onExpandFileContext}
        pendingStyle={diffMountedRowsPlaceholderStyle(window.rows)}
        searchLineKeys={props.searchLineKeys}
      />
      {window.paddingBottom > 0 ? <div aria-hidden="true" className="diff-virtual-spacer" style={window.bottomSpacerStyle} /> : null}
    </div>
  );
}

// DiffRowsHtml mounts cached diff rows as one HTML block.
// DiffRowsHtml 将缓存后的 diff 行作为单个 HTML 块挂载。
function DiffRowsHtml(props: {
  // Rendered HTML for the mounted row window.
  // 已挂载行窗口或整文件对应的渲染 HTML。
  html: string;
  // Stable key for DOM managers that depend on view and overflow mode outside raw HTML identity.
  // DOM 管理器稳定键，用于覆盖仅靠原始 HTML 身份无法表达的视图和溢出模式变化。
  domSetupKey: string;
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行键。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行键。
  activeSearchLineKey?: string | null;
  // Stable placeholder style used while worker HTML is pending.
  // Worker HTML 等待期间使用的稳定占位样式。
  pendingStyle: React.CSSProperties;
  // Optional handler for delegated hunk context expansion controls.
  // hunk 上下文展开控件使用的可选委托处理器。
  onExpandFileContext?: (hunkIndex: number, direction: DiffHunkExpansionDirection) => void;
}) {
  const { locale, t } = useI18n();
  const hostRef = React.useRef<HTMLDivElement | null>(null);
  useTargetDiffDomSetup(hostRef, props.domSetupKey, props.html);
  React.useLayoutEffect(() => {
    applyDiffSearchMarkers(hostRef.current, props.searchLineKeys, props.activeSearchLineKey);
  }, [props.activeSearchLineKey, props.html, props.searchLineKeys]);
  // Worker HTML stays locale-neutral; label its actual interactive controls at the mounted boundary.
  // Worker HTML 保持语言无关，在挂载边界为实际交互控件添加本地化名称。
  React.useLayoutEffect(() => {
    hostRef.current?.querySelectorAll<HTMLElement>("[data-expand-button]").forEach((control) => {
      control.setAttribute("aria-label", control.hasAttribute("data-expand-up")
        ? t("diff.expandContextAbove")
        : control.hasAttribute("data-expand-down")
          ? t("diff.expandContextBelow")
          : t("diff.expandContext"));
    });
  }, [locale, props.html]);
  // Enter and Space reuse the click dispatcher so directional hunk identities cannot diverge.
  // Enter 与空格复用点击分发，避免方向和 hunk 标识出现两套语义。
  const handleKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    if (!(event.target instanceof HTMLElement) || !event.target.matches("[data-expand-button],[data-unmodified-lines]")) return;
    event.preventDefault();
    event.stopPropagation();
    event.target.click();
  }, []);
  const handleClick = React.useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    const clickedElement = event.target instanceof Element ? event.target : null;
    const expandElement = clickedElement ? targetDiffLineInfoElement(clickedElement, hostRef.current) : null;
    if (!expandElement) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const hunkRoot = expandElement.closest<HTMLElement>("[data-expand-index]");
    const hunkWrapper = expandElement.closest<HTMLElement>("[data-separator-wrapper]");
    const fallbackHunkIndex = parseDiffExpandIndex(hunkRoot?.getAttribute("data-expand-index"));
    const direction = normalizedHunkExpansionDirection(
      expandElement.hasAttribute("data-expand-up")
        ? "up"
        : expandElement.hasAttribute("data-expand-down")
          ? "down"
          : "both",
    );
    if (!direction) {
      return;
    }
    if (direction === "up") {
      const hunkIndex = parseDiffExpandIndex(expandElement.getAttribute("data-expand-hunk-index"))
        ?? parseDiffExpandIndex(hunkWrapper?.getAttribute("data-expand-up-index"))
        ?? parseDiffExpandIndex(hunkRoot?.getAttribute("data-expand-up-index"))
        ?? fallbackHunkIndex;
      if (hunkIndex != null) {
        props.onExpandFileContext?.(hunkIndex, "up");
      }
      return;
    }
    if (direction === "down") {
      const hunkIndex = parseDiffExpandIndex(expandElement.getAttribute("data-expand-hunk-index"))
        ?? parseDiffExpandIndex(hunkWrapper?.getAttribute("data-expand-down-index"))
        ?? parseDiffExpandIndex(hunkRoot?.getAttribute("data-expand-down-index"))
        ?? fallbackHunkIndex;
      if (hunkIndex != null) {
        props.onExpandFileContext?.(hunkIndex, "down");
      }
      return;
    }
    const upHunkIndex = parseDiffExpandIndex(hunkWrapper?.getAttribute("data-expand-up-index"))
      ?? parseDiffExpandIndex(hunkRoot?.getAttribute("data-expand-up-index"));
    const downHunkIndex = parseDiffExpandIndex(hunkWrapper?.getAttribute("data-expand-down-index"))
      ?? parseDiffExpandIndex(hunkRoot?.getAttribute("data-expand-down-index"));
    if (upHunkIndex != null && downHunkIndex != null) {
      props.onExpandFileContext?.(upHunkIndex, "both");
      return;
    }
    if (upHunkIndex != null) {
      props.onExpandFileContext?.(upHunkIndex, "up");
      return;
    }
    if (downHunkIndex != null) {
      props.onExpandFileContext?.(downHunkIndex, "down");
      return;
    }
    if (fallbackHunkIndex != null) {
      props.onExpandFileContext?.(fallbackHunkIndex, "both");
    }
  }, [props.onExpandFileContext]);
  if (!props.html && diffPlaceholderStyleHeight(props.pendingStyle) > 0) {
    return (
      <div className="diff-plain-rendered-window" onClick={handleClick} ref={hostRef}>
        <div aria-hidden="true" className="diff-render-placeholder" style={props.pendingStyle} />
      </div>
    );
  }
  return <div className="diff-plain-rendered-window" dangerouslySetInnerHTML={{ __html: props.html }} onClick={handleClick} onKeyDown={handleKeyDown} ref={hostRef} />;
}

// useTargetDiffDomSetup installs target DOM managers and retries when cached HTML swaps its inner structure.
// useTargetDiffDomSetup 安装目标 DOM 管理器，并在缓存 HTML 替换内部结构时重试绑定。
function useTargetDiffDomSetup(
  // Rendered HTML host element reference.
  // 已渲染 HTML 宿主元素引用。
  hostRef: React.RefObject<HTMLDivElement | null>,
  // Stable key for mode changes that require rebinding DOM managers.
  // 需要重新绑定 DOM 管理器的模式变化稳定键。
  domSetupKey: string,
  // Rendered HTML identity used by React to update the host subtree.
  // React 用于更新宿主子树的已渲染 HTML 身份。
  html: string,
): void {
  React.useLayoutEffect(() => {
    const host = hostRef.current;
    if (!host) {
      return undefined;
    }
    let cleanup: (() => void) | undefined;
    let frameId = -1;
    // install refreshes all target DOM managers after React has committed the current subtree.
    // install 在 React 提交当前子树后刷新所有目标 DOM 管理器。
    const install = () => {
      cleanup?.();
      cleanup = setupTargetDiffDom(host);
    };
    // scheduleInstall coalesces repeated structure and size events into one animation frame.
    // scheduleInstall 将连续结构和尺寸事件合并到一个动画帧中执行。
    const scheduleInstall = () => {
      if (frameId !== -1) {
        return;
      }
      frameId = window.requestAnimationFrame(() => {
        frameId = -1;
        install();
      });
    };
    install();
    const mutationObserver = typeof MutationObserver === "undefined"
      ? null
      : new MutationObserver(scheduleInstall);
    mutationObserver?.observe(host, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["data-diff-type", "data-overflow"],
    });
    return () => {
      mutationObserver?.disconnect();
      if (frameId !== -1) {
        window.cancelAnimationFrame(frameId);
      }
      cleanup?.();
    };
  }, [domSetupKey, hostRef, html]);
}

// applyDiffSearchMarkers updates search markers on mounted rows without regenerating diff HTML.
// applyDiffSearchMarkers 在已挂载行上更新搜索标记，避免重新生成 diff HTML。
function applyDiffSearchMarkers(
  // Root element containing the rendered diff HTML.
  // 包含已渲染 diff HTML 的根元素。
  root: HTMLElement | null,
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行键。
  searchLineKeys: ReadonlySet<string> | undefined,
  // Active search result line key.
  // 当前活跃搜索命中行键。
  activeSearchLineKey: string | null | undefined,
): void {
  if (!root) {
    return;
  }
  root.querySelectorAll<HTMLElement>("[data-search-match],[data-search-active]").forEach((element) => {
    element.removeAttribute("data-search-match");
    element.removeAttribute("data-search-active");
  });
  if (!searchLineKeys?.size && !activeSearchLineKey) {
    return;
  }
  root.querySelectorAll<HTMLElement>("[data-diff-line-key]").forEach((element) => {
    const lineKey = element.getAttribute("data-diff-line-key");
    if (!lineKey) {
      return;
    }
    if (searchLineKeys?.has(lineKey)) {
      element.setAttribute("data-search-match", "");
    }
    if (activeSearchLineKey === lineKey) {
      element.setAttribute("data-search-active", "");
    }
  });
}

// targetDiffLineInfoElement mirrors the target event walk for line-info expansion controls.
// targetDiffLineInfoElement 复刻目标 line-info 展开控件的事件向上遍历。
function targetDiffLineInfoElement(start: Element, host: HTMLElement | null): HTMLElement | null {
  let element: Element | null = start;
  while (element && element !== host) {
    if (element instanceof HTMLElement && (
      element.hasAttribute("data-expand-button")
      || element.hasAttribute("data-unmodified-lines")
      || element.hasAttribute("data-separator-controls")
    )) {
      return element;
    }
    element = element.parentElement ?? (element.parentNode instanceof Element ? element.parentNode : null);
  }
  return null;
}

// normalizedHunkExpansionDirection validates delegated hunk direction data.
// normalizedHunkExpansionDirection 校验委托事件中的 hunk 方向数据。
function normalizedHunkExpansionDirection(value: string | undefined): DiffHunkExpansionDirection | null {
  if (value === "up" || value === "down" || value === "both") {
    return value;
  }
  return null;
}

// parseDiffExpandIndex parses delegated separator hunk indexes.
// parseDiffExpandIndex 解析委托分隔条上的 hunk 索引。
function parseDiffExpandIndex(value: string | null | undefined): number | null {
  if (value == null || value.trim() === "") {
    return null;
  }
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : null;
}

// diffFileLoadedRowsHeight returns the fixed-pixel height for loaded hunks in one file.
// diffFileLoadedRowsHeight 返回单个文件已加载 hunk 的固定像素高度。
function diffFileLoadedRowsHeight(file: DiffFile): number {
  const cached = diffFileLoadedRowsHeightCache.get(file);
  if (cached != null) {
    return cached;
  }
  let height = 0;
  for (const hunk of file.hunks) {
    height += DIFF_HUNK_HEADER_HEIGHT + hunk.lines.length * DIFF_LINE_HEIGHT;
  }
  diffFileLoadedRowsHeightCache.set(file, height);
  return height;
}

// diffFileLoadedRowsPlaceholderStyle returns a stable pending placeholder style for a full file render.
// diffFileLoadedRowsPlaceholderStyle 返回整文件渲染使用的稳定等待占位样式。
function diffFileLoadedRowsPlaceholderStyle(file: DiffFile): React.CSSProperties {
  const cached = diffFileLoadedRowsPlaceholderStyleCache.get(file);
  if (cached) {
    return cached;
  }
  const style = { height: diffFileLoadedRowsHeight(file) };
  diffFileLoadedRowsPlaceholderStyleCache.set(file, style);
  return style;
}

// diffMountedRowsHeight returns the fixed-pixel height for one mounted virtual window.
// diffMountedRowsHeight 返回一个已挂载虚拟窗口的固定像素高度。
function diffMountedRowsHeight(rows: readonly DiffMountedVirtualRow[]): number {
  const cached = diffMountedRowsHeightCache.get(rows);
  if (cached != null) {
    return cached;
  }
  let height = 0;
  for (const row of rows) {
    height += row.height;
  }
  diffMountedRowsHeightCache.set(rows, height);
  return height;
}

// diffMountedRowsPlaceholderStyle returns a stable pending placeholder style for one mounted row window.
// diffMountedRowsPlaceholderStyle 返回单个挂载行窗口使用的稳定等待占位样式。
function diffMountedRowsPlaceholderStyle(rows: readonly DiffMountedVirtualRow[]): React.CSSProperties {
  const cached = diffMountedRowsPlaceholderStyleCache.get(rows);
  if (cached) {
    return cached;
  }
  const style = { height: diffMountedRowsHeight(rows) };
  diffMountedRowsPlaceholderStyleCache.set(rows, style);
  return style;
}

// diffPlaceholderStyleHeight reads the numeric height from a cached placeholder style.
// diffPlaceholderStyleHeight 从缓存占位样式中读取数字高度。
function diffPlaceholderStyleHeight(style: React.CSSProperties): number {
  return typeof style.height === "number" ? style.height : 0;
}

// useDiffScrollState tracks the scroll viewport for the diff file list.
// useDiffScrollState 跟踪 diff 文件列表的滚动视口。
export function useDiffScrollState(
  // Scrollable diff file-list element reference.
  // 可滚动 diff 文件列表元素引用。
  containerRef: React.RefObject<HTMLDivElement | null>,
): [DiffScrollState, React.UIEventHandler<HTMLDivElement>, () => void] {
  const frameRef = React.useRef<number | null>(null);
  const [state, setState] = React.useState<DiffScrollState>({ scrollTop: 0, clientHeight: 0, revision: 0 });

  const updateState = React.useCallback(() => {
    const element = containerRef.current;
    if (!element) {
      return;
    }
    setState((previous) => {
      const nextScrollTop = quantizedDiffScrollTop(element.scrollTop);
      const nextClientHeight = quantizedDiffClientHeight(element.clientHeight);
      if (previous.scrollTop === nextScrollTop && previous.clientHeight === nextClientHeight) {
        return previous;
      }
      return {
        scrollTop: nextScrollTop,
        clientHeight: nextClientHeight,
        revision: previous.revision + 1,
      };
    });
  }, [containerRef]);

  const scheduleUpdate = React.useCallback(() => {
    if (frameRef.current != null) {
      return;
    }
    frameRef.current = window.requestAnimationFrame(() => {
      frameRef.current = null;
      updateState();
    });
  }, [updateState]);

  React.useLayoutEffect(() => {
    updateState();
    const element = containerRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return undefined;
    }
    const observer = new ResizeObserver(scheduleUpdate);
    observer.observe(element);
    scheduleUpdate();
    return () => {
      observer.disconnect();
      if (frameRef.current != null) {
        window.cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
    };
  }, [containerRef, scheduleUpdate, updateState]);

  return [state, scheduleUpdate, updateState];
}

// quantizedDiffScrollTop coalesces pixel-level scroll changes while keeping mounted windows conservative.
// quantizedDiffScrollTop 合并像素级滚动变化，同时保持挂载窗口保守覆盖。
function quantizedDiffScrollTop(scrollTop: number): number {
  return Math.floor(Math.max(0, scrollTop) / DIFF_SCROLL_STATE_GRANULARITY_PX) * DIFF_SCROLL_STATE_GRANULARITY_PX;
}

// quantizedDiffClientHeight expands viewport height to avoid under-mounting after resize rounding.
// quantizedDiffClientHeight 放大视口高度，避免 resize 取整后挂载不足。
function quantizedDiffClientHeight(clientHeight: number): number {
  if (clientHeight <= 0) {
    return 0;
  }
  return Math.ceil(clientHeight / DIFF_SCROLL_STATE_GRANULARITY_PX) * DIFF_SCROLL_STATE_GRANULARITY_PX;
}

// displayPath returns the primary file path shown in the file header.
// displayPath 返回文件头中展示的主路径。
function displayPath(file: DiffFile): string {
  return file.newPath && file.newPath !== "/dev/null" ? file.newPath : file.oldPath;
}

// nextExpandedContextLineCount returns the next bounded Git context size for one file.
// nextExpandedContextLineCount 返回单个文件下一档受限 Git 上下文大小。
function nextExpandedContextLineCount(file: DiffFile): number {
  const currentContextLineCount = file.contextLineCount ?? DIFF_DEFAULT_CONTEXT_LINES;
  return Math.max(DIFF_CONTEXT_EXPANSION_INCREMENT, currentContextLineCount + DIFF_CONTEXT_EXPANSION_INCREMENT);
}

// canExpandDiffFileContext returns whether another larger Git context step exists.
// canExpandDiffFileContext 返回是否还存在更大的 Git 上下文档位。
function canExpandDiffFileContext(file: DiffFile): boolean {
  return !file.tooLargeToDisplay;
}
