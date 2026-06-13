import React from "react";
import { ChevronDown, ChevronRight } from "lucide-react";

import type { DiffFile, DiffLine } from "./types";
import {
  DIFF_HIGHLIGHT_ROW_LIMIT,
  diffLanguageForFile,
  escapeHtml,
  highlightedDiffLineHtml,
  useHighlightLanguage,
} from "./diff-highlight";
import {
  DIFF_VIRTUALIZATION_THRESHOLD,
  type DiffScrollState,
  buildDiffVirtualLayout,
  elementTopWithinScrollContainer,
  fileTotalRowCount,
  mountedDiffRows,
} from "./diff-virtual-layout";

// DiffFileCardProps stores render inputs for one memoized file card.
// DiffFileCardProps 保存单个记忆化文件卡片的渲染输入。
interface DiffFileCardProps {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
}

// DiffFileCard renders one file-level diff card.
// DiffFileCard 渲染单个文件级 diff 卡片。
export const DiffFileCard = React.memo(function DiffFileCard(props: DiffFileCardProps) {
  const [collapsed, setCollapsed] = React.useState(false);
  const ToggleIcon = collapsed ? ChevronRight : ChevronDown;
  const rowCount = React.useMemo(() => fileTotalRowCount(props.file), [props.file]);
  const shouldHighlight = rowCount <= DIFF_HIGHLIGHT_ROW_LIMIT;
  const language = !collapsed && shouldHighlight ? diffLanguageForFile(props.file) : null;
  const highlightReady = useHighlightLanguage(language);
  const handleToggle = React.useCallback(() => {
    setCollapsed((previous) => !previous);
  }, []);
  return (
    <article className={`diff-file-card ${props.file.status}`}>
      <button className="diff-file-header" onClick={handleToggle} type="button">
        <ToggleIcon size={15} />
        <div className="diff-file-paths">
          <strong>
            <span>{displayPath(props.file)}</span>
            {props.file.status === "added" ? <span aria-label="新增文件" className="diff-file-status-dot" /> : null}
          </strong>
          {props.file.status === "renamed" ? <span>{`${props.file.oldPath} -> ${props.file.newPath}`}</span> : null}
        </div>
        <div className="diff-file-meta">
          <span className="diff-stat added">+{props.file.additions}</span>
          <span className="diff-stat removed">-{props.file.deletions}</span>
        </div>
      </button>
      {!collapsed ? (
        <div className="diff-file-body">
          {props.file.isBinary ? (
            <div className="diff-binary-note">二进制文件变更，未渲染文本内容。</div>
          ) : props.file.hunks.length ? (
            <DiffFileLines
              file={props.file}
              highlightReady={highlightReady}
              language={language}
              rowCount={rowCount}
              scrollContainerRef={props.scrollContainerRef}
              scrollState={props.scrollState}
            />
          ) : (
            <div className="diff-binary-note">该文件没有可渲染的文本 hunk。</div>
          )}
        </div>
      ) : null}
    </article>
  );
}, areDiffFileCardPropsEqual);

// DiffFileLines switches between direct and virtualized rendering for one file.
// DiffFileLines 为单个文件在直接渲染和虚拟渲染之间切换。
function DiffFileLines(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the language module is registered and ready.
  // 语言模块是否已经注册并可用。
  highlightReady: boolean;
  // Precomputed visual row count for this file.
  // 该文件预先计算的可视行数。
  rowCount: number;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
}) {
  if (props.rowCount < DIFF_VIRTUALIZATION_THRESHOLD) {
    return (
      <>
        {props.file.hunks.map((hunk, index) => (
          <section className="diff-hunk" key={`${props.file.key}-hunk-${index}`}>
            <div className="diff-hunk-header">{hunk.header}</div>
            <div className="diff-lines">
              {hunk.lines.map((line, lineIndex) => (
                <DiffLineRow
                  highlightReady={props.highlightReady}
                  key={`${props.file.key}-${index}-${lineIndex}`}
                  language={props.language}
                  line={line}
                />
              ))}
            </div>
          </section>
        ))}
      </>
    );
  }
  return (
    <VirtualDiffFileLines
      file={props.file}
      highlightReady={props.highlightReady}
      language={props.language}
      scrollContainerRef={props.scrollContainerRef}
      scrollState={props.scrollState}
    />
  );
}

// areDiffFileCardPropsEqual avoids re-rendering unaffected file cards during scroll or collapse.
// areDiffFileCardPropsEqual 避免滚动或折叠时重渲染未受影响的文件卡片。
function areDiffFileCardPropsEqual(previous: DiffFileCardProps, next: DiffFileCardProps): boolean {
  if (
    previous.file !== next.file
    || previous.scrollContainerRef !== next.scrollContainerRef
  ) {
    return false;
  }
  if (fileTotalRowCount(next.file) < DIFF_VIRTUALIZATION_THRESHOLD) {
    return true;
  }
  return previous.scrollState.scrollTop === next.scrollState.scrollTop
    && previous.scrollState.clientHeight === next.scrollState.clientHeight
    && previous.scrollState.revision === next.scrollState.revision;
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
  // Whether the language module is registered and ready.
  // 语言模块是否已经注册并可用。
  highlightReady: boolean;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
}) {
  const hostRef = React.useRef<HTMLDivElement | null>(null);
  const [hostTop, setHostTop] = React.useState(0);
  const layout = React.useMemo(() => buildDiffVirtualLayout(props.file), [props.file]);
  const window = React.useMemo(
    () => mountedDiffRows(props.file, layout, props.scrollState.scrollTop - hostTop, props.scrollState.clientHeight),
    [hostTop, layout, props.file, props.scrollState.clientHeight, props.scrollState.scrollTop],
  );

  React.useLayoutEffect(() => {
    const host = hostRef.current;
    const container = props.scrollContainerRef.current;
    if (!host || !container) {
      return undefined;
    }
    const nextTop = elementTopWithinScrollContainer(host, container);
    setHostTop((previousTop) => (Math.abs(previousTop - nextTop) > 0.5 ? nextTop : previousTop));
    const frame = globalThis.requestAnimationFrame(() => {
      const currentHost = hostRef.current;
      const currentContainer = props.scrollContainerRef.current;
      if (!currentHost || !currentContainer) {
        return;
      }
      const delayedTop = elementTopWithinScrollContainer(currentHost, currentContainer);
      setHostTop((previousTop) => (Math.abs(previousTop - delayedTop) > 0.5 ? delayedTop : previousTop));
    });
    return () => globalThis.cancelAnimationFrame(frame);
  }, [layout.totalHeight, props.scrollContainerRef, props.scrollState.revision]);

  return (
    <div className="diff-virtual-lines" ref={hostRef}>
      {window.paddingTop > 0 ? <div aria-hidden="true" className="diff-virtual-spacer" style={{ height: window.paddingTop }} /> : null}
      {window.rows.map((row) => (
        row.kind === "hunk" ? (
          <div className="diff-hunk-header virtual" key={row.key}>{row.header}</div>
        ) : (
          <DiffLineRow
            highlightReady={props.highlightReady}
            key={row.key}
            language={props.language}
            line={row.line}
          />
        )
      ))}
      {window.paddingBottom > 0 ? <div aria-hidden="true" className="diff-virtual-spacer" style={{ height: window.paddingBottom }} /> : null}
    </div>
  );
}

// DiffLineRow renders one memoized line-level diff row.
// DiffLineRow 渲染单个带记忆化的行级 diff 行。
const DiffLineRow = React.memo(function DiffLineRow(props: {
  // Parsed line data.
  // 已解析行数据。
  line: DiffLine;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the language module is registered and ready.
  // 语言模块是否已经注册并可用。
  highlightReady: boolean;
}) {
  const visualLineNumber = props.line.kind === "removed" ? props.line.oldLine : props.line.newLine;
  const highlightedHtml = React.useMemo(
    () => (!props.highlightReady || props.line.kind === "metadata" || !props.language
      ? escapeHtml(props.line.content || " ")
      : highlightedDiffLineHtml(props.line.content || " ", props.language)),
    [props.highlightReady, props.language, props.line.content, props.line.kind],
  );
  return (
    <div className={`diff-line-row ${props.line.kind}`}>
      <span className="diff-line-number">{formatLineNumber(visualLineNumber)}</span>
      <code dangerouslySetInnerHTML={{ __html: highlightedHtml }} />
    </div>
  );
});

// useDiffScrollState tracks the scroll viewport for the diff file list.
// useDiffScrollState 跟踪 diff 文件列表的滚动视口。
export function useDiffScrollState(containerRef: React.RefObject<HTMLDivElement | null>): [DiffScrollState, React.UIEventHandler<HTMLDivElement>] {
  const frameRef = React.useRef<number | null>(null);
  const [state, setState] = React.useState<DiffScrollState>({ scrollTop: 0, clientHeight: 0, revision: 0 });

  const updateState = React.useCallback(() => {
    const element = containerRef.current;
    if (!element) {
      return;
    }
    setState((previous) => {
      const nextScrollTop = element.scrollTop;
      const nextClientHeight = element.clientHeight;
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

  return [state, scheduleUpdate];
}

// displayPath returns the primary file path shown in the file header.
// displayPath 返回文件头中展示的主路径。
function displayPath(file: DiffFile): string {
  return file.newPath && file.newPath !== "/dev/null" ? file.newPath : file.oldPath;
}

// formatLineNumber formats optional old and new line numbers for the gutter.
// formatLineNumber 格式化 gutter 中可选的新旧行号。
function formatLineNumber(line: number | null): string {
  return line == null ? "" : String(line);
}
