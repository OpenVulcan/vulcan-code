import * as React from "react";

import {
  diffLanguageForFile,
  useHighlightedDiffFileRevision,
  useHighlightLanguage,
} from "@/diff/diff-highlight";
import {
  useHighlightedDiffFileHtml,
  usePlainDiffFileHtml,
} from "@/diff/diff-render-worker-client";
import type { DiffFile, DiffHunkSeparatorStyle } from "@/diff/types";
import "@/diff/diffReview.css";

// ChangeSetDiffPreview renders one change-set file through the shared review diff renderer.
// ChangeSetDiffPreview 通过共享代码审核 diff 渲染器渲染单个 change-set 文件。
export function ChangeSetDiffPreview(props: {
  // Parsed diff file adapted from a change-set payload.
  // 从 change-set 载荷适配出的已解析 diff 文件。
  file: DiffFile;
  // Separator style used between rendered hunks.
  // 渲染 hunk 之间使用的分隔样式。
  hunkSeparatorStyle?: DiffHunkSeparatorStyle;
  // Whether inline change emphasis should be enabled.
  // 是否启用行内变更强调。
  inlineDiff?: boolean;
}) {
  const language = React.useMemo(() => diffLanguageForFile(props.file), [props.file]);
  const highlightSupported = useHighlightLanguage(language);
  const useHighlightedRenderer = Boolean(language && highlightSupported);
  const highlightRevision = useHighlightedDiffFileRevision(props.file, language, useHighlightedRenderer);
  const plainHtml = usePlainDiffFileHtml({
    activeSearchLineKey: null,
    contextControls: false,
    file: props.file,
    hunkSeparatorStyle: props.hunkSeparatorStyle ?? "line-info",
    indicatorStyle: "bars",
    inlineDiff: props.inlineDiff ?? true,
    overflowMode: "scroll",
    viewMode: "unified",
  }, !useHighlightedRenderer);
  const highlightedHtml = useHighlightedDiffFileHtml({
    activeSearchLineKey: null,
    contextControls: false,
    file: props.file,
    highlightRevision,
    hunkSeparatorStyle: props.hunkSeparatorStyle ?? "line-info",
    indicatorStyle: "bars",
    inlineDiff: props.inlineDiff ?? true,
    language: language ?? "text",
    overflowMode: "scroll",
    viewMode: "unified",
  }, useHighlightedRenderer);
  const html = useHighlightedRenderer ? highlightedHtml : plainHtml;
  return (
    <div className="file-change-review-preview diff-plain-rendered-window">
      <div dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  );
}
