import React from "react";
import { diffScrollTopForLine, type DiffFileVirtualLayout } from "../diff-render-index";
import { diffSearchLineKey } from "../diff-search";
import type { DiffFile } from "../types";

// DiffChangeTarget is the first line of one run of added or removed lines.
// DiffChangeTarget 是一段连续新增或删除行的首行。
export interface DiffChangeTarget {
  hunkIndex: number;
  lineIndex: number;
}

// DiffChangeCursor remembers the change the last step showed and where that left the view, so a step that follows
// without scrolling in between moves by one change even where position estimates are coarse (split view).
// DiffChangeCursor 记住上一次步进展示的变更及其后的视图位置；中间未滚动时，即使位置估算较粗（分栏视图）也只移动一处变更。
export interface DiffChangeCursor {
  fileKey: string;
  index: number;
  scrollTop: number;
}

// diffChangeTargets lists where each change starts in a file's loaded hunks: a change is a run of added or removed
// lines, broken only by context lines ("\ No newline" metadata belongs to the run around it).
// diffChangeTargets 列出文件已加载 hunk 中每处变更的起点：变更是连续的新增或删除行，只被上下文行断开（"\ No newline" 元数据归属其所在段）。
export function diffChangeTargets(file: DiffFile): DiffChangeTarget[] {
  const targets: DiffChangeTarget[] = [];
  file.hunks.forEach((hunk, hunkIndex) => {
    let inRun = false;
    hunk.lines.forEach((line, lineIndex) => {
      if (line.kind === "context") inRun = false;
      else if (line.kind !== "metadata") {
        if (!inRun) targets.push({ hunkIndex, lineIndex });
        inRun = true;
      }
    });
  });
  return targets;
}

// diffChangeStepIndex picks the change a step lands on. Right after a step it moves by one; once the view has
// been scrolled it takes the first change below the top of the view, or the last one above it, so stepping
// resumes from what is on screen. Both directions wrap.
// diffChangeStepIndex 选出步进落到的变更。紧接上一次步进时移动一处；视图滚动过后，向下取视图顶部以下的第一处，向上取其上方的最后一处，
// 使步进从屏幕所见处继续。两个方向都循环。
export function diffChangeStepIndex(
  // Estimated top of each change in the scroll container, in change order.
  // 每处变更在滚动容器中的估算顶部，按变更顺序排列。
  tops: readonly number[],
  scrollTop: number,
  cursor: Pick<DiffChangeCursor, "index" | "scrollTop"> | null,
  direction: 1 | -1,
): number {
  const count = tops.length;
  if (!count) return -1;
  if (cursor && cursor.index < count && Math.abs(cursor.scrollTop - scrollTop) <= 2) {
    return (cursor.index + direction + count) % count;
  }
  if (direction === 1) {
    const below = tops.findIndex((top) => top > scrollTop + 1);
    return below < 0 ? 0 : below;
  }
  for (let index = count - 1; index >= 0; index -= 1) {
    if (tops[index] < scrollTop - 1) return index;
  }
  return count - 1;
}

// useDiffChangeNavigation steps the review's file list through the shown file's changes, centring each one.
// useDiffChangeNavigation 让审核文件列表在当前文件的变更间步进，并把每处变更居中。
export function useDiffChangeNavigation(
  listRef: React.RefObject<HTMLDivElement | null>,
  layout: DiffFileVirtualLayout,
  file: DiffFile | null,
): { count: number; step: (direction: 1 | -1) => void } {
  const targets = React.useMemo(() => (file ? diffChangeTargets(file) : []), [file]);
  const cursorRef = React.useRef<DiffChangeCursor | null>(null);
  const step = React.useCallback((direction: 1 | -1) => {
    const list = listRef.current;
    if (!list || !file) return;
    const tops = targets.map((target) => diffScrollTopForLine(layout, file.key, target.hunkIndex, target.lineIndex, 0) ?? 0);
    const cursor = cursorRef.current?.fileKey === file.key ? cursorRef.current : null;
    const index = diffChangeStepIndex(tops, list.scrollTop, cursor, direction);
    if (index < 0) return;
    list.scrollTop = Math.max(0, tops[index] - list.clientHeight / 2);
    const { hunkIndex, lineIndex } = targets[index];
    cursorRef.current = { fileKey: file.key, index, scrollTop: list.scrollTop };
    // The estimate mounts the rows; the rendered line then centres exactly and is marked where it is found.
    // 估算位置使行被挂载；找到渲染后的行时再精确居中并加标记。
    window.requestAnimationFrame(() => {
      const line = list.querySelector<HTMLElement>(`[data-diff-line-key="${diffSearchLineKey(file.key, hunkIndex, lineIndex)}"]`);
      list.querySelector("[data-change-focus]")?.removeAttribute("data-change-focus");
      line?.setAttribute("data-change-focus", "");
      line?.scrollIntoView({ block: "center" });
      if (cursorRef.current?.index === index) cursorRef.current.scrollTop = list.scrollTop;
    });
  }, [file, layout, listRef, targets]);
  return { count: targets.length, step };
}
