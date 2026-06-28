import type { DiffFile, DiffLine } from "../types";
import type { TargetDiffContent, TargetDiffFile, TargetDiffHunk } from "./types";

// buildTargetDiffFile converts the project parser output into target hunkContent form.
// buildTargetDiffFile 将项目解析器输出转换为目标 hunkContent 形态。
export function buildTargetDiffFile(file: DiffFile): TargetDiffFile {
  const additionLines: DiffLine[] = [];
  const deletionLines: DiffLine[] = [];
  const hunks: TargetDiffHunk[] = [];
  let nextSplitLine = 0;
  let nextUnifiedLine = 0;
  let previousAdditionEnd = 0;

  for (const hunk of file.hunks) {
    const hunkHeader = parseTargetHunkHeader(hunk.header, hunk.oldStart, hunk.newStart);
    const targetHunk: TargetDiffHunk = {
      header: hunk.header,
      collapsedBefore: Math.max(hunkHeader.additionStart - 1 - previousAdditionEnd, 0),
      splitLineStart: 0,
      splitLineCount: 0,
      unifiedLineStart: 0,
      unifiedLineCount: 0,
      deletionStart: hunkHeader.deletionStart,
      additionStart: hunkHeader.additionStart,
      deletionCount: hunkHeader.deletionCount,
      additionCount: hunkHeader.additionCount,
      deletionLines: 0,
      additionLines: 0,
      deletionLineIndex: deletionLines.length,
      additionLineIndex: additionLines.length,
      hunkContent: [],
      noEOFCRAdditions: false,
      noEOFCRDeletions: false,
    };

    appendTargetHunkContent(targetHunk, hunk.lines, additionLines, deletionLines);
    for (const content of targetHunk.hunkContent) {
      if (content.type === "context") {
        targetHunk.splitLineCount += content.lines;
        targetHunk.unifiedLineCount += content.lines;
      } else {
        targetHunk.splitLineCount += Math.max(content.additions, content.deletions);
        targetHunk.unifiedLineCount += content.additions + content.deletions;
      }
    }
    targetHunk.splitLineStart = nextSplitLine + targetHunk.collapsedBefore;
    targetHunk.unifiedLineStart = nextUnifiedLine + targetHunk.collapsedBefore;
    nextSplitLine += targetHunk.collapsedBefore + targetHunk.splitLineCount;
    nextUnifiedLine += targetHunk.collapsedBefore + targetHunk.unifiedLineCount;
    previousAdditionEnd = targetHunk.additionStart + targetHunk.additionCount - 1;
    hunks.push(targetHunk);
  }

  return {
    source: file,
    name: file.newPath,
    prevName: file.status === "renamed" ? file.oldPath : undefined,
    hunks,
    additionLines,
    deletionLines,
    splitLineCount: nextSplitLine,
    unifiedLineCount: nextUnifiedLine,
    isPartial: Boolean(file.isPartial) || file.fileDiffLoadState?.status === "loading",
  };
}

// appendTargetHunkContent groups raw hunk lines into target context/change blocks.
// appendTargetHunkContent 将原始 hunk 行分组为目标 context/change 块。
function appendTargetHunkContent(
  targetHunk: TargetDiffHunk,
  lines: readonly DiffLine[],
  additionLines: DiffLine[],
  deletionLines: DiffLine[],
): void {
  let activeContent: TargetDiffContent | null = null;
  for (let sourceLineIndex = 0; sourceLineIndex < lines.length; sourceLineIndex += 1) {
    const line = lines[sourceLineIndex];
    if (line.kind === "added") {
      targetHunk.noEOFCRAdditions = targetHunk.noEOFCRAdditions || targetNoEofLine(line);
      if (activeContent?.type !== "change") {
        activeContent = {
          type: "change",
          additions: 0,
          deletions: 0,
          additionLineIndex: additionLines.length,
          deletionLineIndex: deletionLines.length,
          additionSourceLineIndexes: [],
          deletionSourceLineIndexes: [],
        };
        targetHunk.hunkContent.push(activeContent);
      }
      additionLines.push(line);
      activeContent.additions += 1;
      activeContent.additionSourceLineIndexes.push(sourceLineIndex);
      targetHunk.additionLines += 1;
      continue;
    }
    if (line.kind === "removed") {
      targetHunk.noEOFCRDeletions = targetHunk.noEOFCRDeletions || targetNoEofLine(line);
      if (activeContent?.type !== "change") {
        activeContent = {
          type: "change",
          additions: 0,
          deletions: 0,
          additionLineIndex: additionLines.length,
          deletionLineIndex: deletionLines.length,
          additionSourceLineIndexes: [],
          deletionSourceLineIndexes: [],
        };
        targetHunk.hunkContent.push(activeContent);
      }
      deletionLines.push(line);
      activeContent.deletions += 1;
      activeContent.deletionSourceLineIndexes.push(sourceLineIndex);
      targetHunk.deletionLines += 1;
      continue;
    }
    if (line.kind === "metadata") {
      targetHunk.noEOFCRAdditions =
        targetHunk.noEOFCRAdditions || line.content.includes("No newline at end of file");
      targetHunk.noEOFCRDeletions =
        targetHunk.noEOFCRDeletions || line.content.includes("No newline at end of file");
      continue;
    }
    if (activeContent?.type !== "context") {
      activeContent = {
        type: "context",
        lines: 0,
        additionLineIndex: additionLines.length,
        deletionLineIndex: deletionLines.length,
        sourceLineIndexes: [],
      };
      targetHunk.hunkContent.push(activeContent);
    }
    additionLines.push(line);
    deletionLines.push(line);
    activeContent.lines += 1;
    activeContent.sourceLineIndexes.push(sourceLineIndex);
    targetHunk.additionLines += 1;
    targetHunk.deletionLines += 1;
  }
}

// parseTargetHunkHeader extracts target-compatible line counts from a hunk header.
// parseTargetHunkHeader 从 hunk 头部提取目标兼容行计数。
function parseTargetHunkHeader(
  header: string,
  fallbackDeletionStart: number,
  fallbackAdditionStart: number,
): {
  // Hunk old-file start line.
  // hunk 旧文件起始行。
  deletionStart: number;
  // Hunk old-file line count.
  // hunk 旧文件行数。
  deletionCount: number;
  // Hunk new-file start line.
  // hunk 新文件起始行。
  additionStart: number;
  // Hunk new-file line count.
  // hunk 新文件行数。
  additionCount: number;
} {
  const match = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(header);
  return {
    deletionStart: match ? Number.parseInt(match[1], 10) : fallbackDeletionStart,
    deletionCount: match?.[2] ? Number.parseInt(match[2], 10) : 1,
    additionStart: match ? Number.parseInt(match[3], 10) : fallbackAdditionStart,
    additionCount: match?.[4] ? Number.parseInt(match[4], 10) : 1,
  };
}

// targetNoEofLine detects parser metadata that marks a no-newline row.
// targetNoEofLine 检测解析器中标记无换行的行。
function targetNoEofLine(line: DiffLine): boolean {
  return line.content === "\\ No newline at end of file" || line.content.includes("No newline at end of file");
}
