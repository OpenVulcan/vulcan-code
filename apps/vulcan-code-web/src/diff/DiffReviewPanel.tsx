import React from "react";
import { FileText, RefreshCw } from "lucide-react";

import { DiffFileCard, useDiffScrollState } from "./diff-file-card";
import { DiffPanelToolbar } from "./diff-review-toolbar";
import type { DiffPanelSnapshot, DiffReviewSelection } from "./types";
import "./diffReview.css";

// DiffReviewPanel renders one structured Git diff snapshot.
// DiffReviewPanel 渲染单个结构化 Git diff 快照。
export function DiffReviewPanel(props: {
  // Parsed diff snapshot to render.
  // 要渲染的已解析 diff 快照。
  snapshot: DiffPanelSnapshot | null;
  // Whether the panel is currently loading.
  // 面板当前是否正在加载。
  loading?: boolean;
  // Current Git review source selection.
  // 当前 Git 审核来源选择。
  selection?: DiffReviewSelection;
  // Optional empty-state title.
  // 可选空状态标题。
  emptyTitle?: string;
  // Optional empty-state body.
  // 可选空状态正文。
  emptyBody?: string;
  // Optional refresh action.
  // 可选刷新动作。
  onRefresh?: () => void;
  // Optional selection change handler.
  // 可选选择变更处理器。
  onSelectionChange?: (selection: DiffReviewSelection) => void;
  // Optional copy action receiving raw diff text.
  // 接收原始 diff 文本的可选复制动作。
  onCopyRaw?: (rawDiff: string) => void;
}) {
  const fileListRef = React.useRef<HTMLDivElement | null>(null);
  const [scrollState, handleFileListScroll] = useDiffScrollState(fileListRef);
  const snapshot = props.snapshot;
  const selection = props.selection ?? { mode: "unstaged" };

  if (!snapshot) {
    return (
      <section className="diff-review-panel empty">
        <div className="diff-empty-state">
          <FileText size={22} />
          <strong>{props.emptyTitle ?? "Git Diff"}</strong>
          <span>{props.emptyBody ?? "当前没有可展示的 Git Diff。"}</span>
          {props.onRefresh ? (
            <button className="diff-action-button" disabled={props.loading} onClick={() => props.onRefresh?.()} type="button">
              <RefreshCw size={14} />
              <span>{props.loading ? "加载中" : "刷新"}</span>
            </button>
          ) : null}
        </div>
      </section>
    );
  }

  return (
    <section className="diff-review-panel">
      <DiffPanelToolbar
        loading={props.loading}
        selection={selection}
        snapshot={snapshot}
        onCopyRaw={props.onCopyRaw}
        onRefresh={props.onRefresh}
        onSelectionChange={props.onSelectionChange}
      />
      <div className="diff-file-list" onScroll={handleFileListScroll} ref={fileListRef}>
        {snapshot.files.length ? snapshot.files.map((file) => (
          <DiffFileCard
            file={file}
            key={file.key}
            scrollContainerRef={fileListRef}
            scrollState={scrollState}
          />
        )) : (
          <div className="diff-empty-state in-list">
            <FileText size={20} />
            <strong>{props.emptyTitle ?? "Git Diff"}</strong>
            <span>{snapshot.notice ?? props.emptyBody ?? "当前没有可展示的 Git Diff。"}</span>
          </div>
        )}
      </div>
    </section>
  );
}
