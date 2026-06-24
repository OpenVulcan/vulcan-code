import { hasRenderableDiff } from "@/diff/parseDiff";
import { parseUnifiedDiffAsync } from "@/diff/diff-parse-worker-client";
import { diffFileLargeReasons, diffStatsLargeReasons } from "@/diff/limits";
import type {
  DiffFile,
  DiffHunk,
  DiffHunkExpansionDirection,
  DiffHunkExpansionRequest,
  DiffLine,
  DiffPanelSnapshot,
  DiffReviewSelection,
  WorkspaceDiffFileDetailResponse,
  WorkspaceDiffFileSummary,
  WorkspaceDiffSnapshotResponse,
} from "@/diff/types";
import type {
  ConversationMessage,
  ConversationSummary,
  ProjectSummary,
  WorkbenchSnapshot,
} from "@/types";

// WorkspaceReviewDraftConversation carries a local draft conversation route.
// WorkspaceReviewDraftConversation 承载本地草稿会话路由。
export interface WorkspaceReviewDraftConversation {
  // Draft conversation id used by the selected route.
  // 当前选中路由使用的草稿会话标识。
  id: string;
  // Project id that owns the draft conversation.
  // 草稿会话所属的项目标识。
  projectId: string;
}

// WorkspaceReviewConversationBlock is the minimal conversation block shape needed by diff extraction.
// WorkspaceReviewConversationBlock 是 diff 提取所需的最小会话块结构。
export interface WorkspaceReviewConversationBlock {
  // Block kind projected from a transcript message.
  // 从转录消息投影出的块类型。
  kind: string;
  // Block body text.
  // 块正文文本。
  body: string;
}

// WorkspaceReviewConversationDiffSource stores a raw conversation diff before worker parsing.
// WorkspaceReviewConversationDiffSource 保存 Worker 解析前的会话原始 diff。
export interface WorkspaceReviewConversationDiffSource {
  // Stable source key used to reject stale async parse results.
  // 用于拒绝过期异步解析结果的稳定来源键。
  key: string;
  // Raw unified diff body extracted from the conversation timeline.
  // 从会话时间线提取的原始 unified diff 正文。
  rawDiff: string;
  // Review panel source derived from message metadata.
  // 从消息元数据推导出的审核面板来源。
  source: DiffPanelSnapshot["source"];
}

// activeWorkbenchSurfaceId returns the currently visible parent or subagent conversation id.
// activeWorkbenchSurfaceId 返回当前可见的父会话或子代理会话标识。
export function activeWorkbenchSurfaceId(workbench: WorkbenchSnapshot, selectedConversationId: string | null | undefined): string | null {
  const parentConversationId = selectedConversationId ?? workbench.selectedConversationId ?? null;
  if (workbench.subagentNavigation?.parentConversationId === parentConversationId) {
    return workbench.subagentNavigation.activeConversationId ?? parentConversationId;
  }
  return parentConversationId;
}

// selectedConversationProjectId returns the explicit project id for the currently visible conversation or draft.
// selectedConversationProjectId 返回当前可见会话或草稿的明确项目标识。
export function selectedConversationProjectId(
  workbench: WorkbenchSnapshot,
  selectedConversationId: string | null | undefined,
  draftConversation?: WorkspaceReviewDraftConversation | null,
): string | null {
  if (draftConversation && selectedConversationId === draftConversation.id) {
    return draftConversation.projectId;
  }
  const activeSurfaceId = activeWorkbenchSurfaceId(workbench, selectedConversationId);
  const parentConversationId = selectedConversationId ?? workbench.selectedConversationId ?? null;
  const conversation = activeSurfaceId
    ? workbench.conversations.find((item) => item.id === activeSurfaceId)
      ?? workbench.conversations.find((item) => item.id === parentConversationId)
    : null;
  if (!conversation?.projectId) {
    return null;
  }
  return workbench.projects.some((project) => project.id === conversation.projectId) ? conversation.projectId : null;
}

// selectedConversationWorkspacePath returns the explicit project path for the currently visible conversation or draft.
// selectedConversationWorkspacePath 返回当前可见会话或草稿的明确项目路径。
export function selectedConversationWorkspacePath(
  workbench: WorkbenchSnapshot,
  selectedConversationId: string | null | undefined,
  draftConversation?: WorkspaceReviewDraftConversation | null,
): string | null {
  if (draftConversation && selectedConversationId === draftConversation.id) {
    return workbench.projects.find((project) => project.id === draftConversation.projectId)?.path ?? null;
  }
  const activeSurfaceId = activeWorkbenchSurfaceId(workbench, selectedConversationId);
  const parentConversationId = selectedConversationId ?? workbench.selectedConversationId ?? null;
  if (!activeSurfaceId) {
    return null;
  }
  const conversation = workbench.conversations.find((item) => item.id === activeSurfaceId)
    ?? workbench.conversations.find((item) => item.id === parentConversationId);
  if (!conversation) {
    return null;
  }
  return projectForConversation(workbench, conversation)?.path ?? null;
}

// projectForConversation resolves a conversation owner project only from an explicit project id.
// projectForConversation 仅从明确项目标识解析会话所属项目。
function projectForConversation(
  workbench: WorkbenchSnapshot,
  conversation: ConversationSummary,
): ProjectSummary | null {
  if (!conversation.projectId) {
    return null;
  }
  return workbench.projects.find((project) => project.id === conversation.projectId) ?? null;
}

// diffSnapshotFromWorkspaceResponseAsync converts backend Git response through the worker-only parser.
// diffSnapshotFromWorkspaceResponseAsync 通过仅 Worker 解析器转换后端 Git 响应。
export async function diffSnapshotFromWorkspaceResponseAsync(response: WorkspaceDiffSnapshotResponse): Promise<DiffPanelSnapshot> {
  const snapshot = await parseUnifiedDiffAsync(response.unifiedDiff, "workspace", response.repositoryRoot);
  return diffSnapshotFromParsedWorkspaceResponse(response, snapshot);
}

// diffSnapshotFromParsedWorkspaceResponse merges backend summary metadata into one parsed snapshot.
// diffSnapshotFromParsedWorkspaceResponse 将后端摘要元数据合并进已解析快照。
function diffSnapshotFromParsedWorkspaceResponse(
  response: WorkspaceDiffSnapshotResponse,
  snapshot: DiffPanelSnapshot,
): DiffPanelSnapshot {
  const files = response.files?.length
    ? mergeWorkspaceDiffSummaries(response.files, snapshot.files, Boolean(response.truncated))
    : snapshot.files;
  const totalChangedLines = response.totalChangedLines ?? snapshot.stats.totalChangedLines;
  const totalChangedBytes = response.totalChangedBytes ?? snapshot.stats.totalChangedBytes;
  const statsBase = {
    files: response.changedFiles || files.length || snapshot.stats.files,
    additions: response.additions || snapshot.stats.additions,
    deletions: response.deletions || snapshot.stats.deletions,
    totalChangedLines,
    totalChangedBytes,
  };
  const largeReasons = diffStatsLargeReasons(statsBase);
  return {
    ...snapshot,
    files,
    notice: response.notice ?? snapshot.notice,
    stats: {
      ...statsBase,
      isLarge: largeReasons.length > 0,
      largeReasons,
    },
    completeness: response.truncated ? "truncated" : snapshot.completeness,
    isTruncated: Boolean(response.truncated) || snapshot.isTruncated,
    currentBranch: response.currentBranch ?? null,
    defaultBranch: response.defaultBranch ?? null,
    selectedBaseBranch: response.selectedBaseBranch ?? null,
    selectedCompareBranch: response.selectedCompareBranch ?? null,
    selectedCommitHash: response.selectedCommitHash ?? null,
    branches: response.branches ?? [],
    recentCommits: response.recentCommits ?? [],
  };
}

// diffFileFromWorkspaceDetailAsync parses one backend file detail through the worker-only parser.
// diffFileFromWorkspaceDetailAsync 通过仅 Worker 解析器解析单个后端文件详情。
export async function diffFileFromWorkspaceDetailAsync(
  response: WorkspaceDiffFileDetailResponse,
  fallbackFile: DiffFile,
  expansion?: DiffHunkExpansionRequest,
): Promise<DiffFile | null> {
  if (response.truncated && response.file?.tooLargeToDisplay && !response.unifiedDiff.trim()) {
    return {
      ...fallbackFile,
      ...workspaceDiffFileFieldsFromSummary(response.file),
      isPartial: true,
      fileDiffLoadState: { status: "loaded", message: response.notice ?? "该文件超过 WebUI 展示阈值，未传输 diff 正文。" },
      contextLineCount: response.contextLines ?? fallbackFile.contextLineCount,
      rawDiff: fallbackFile.rawDiff,
      hunks: [],
    };
  }
  const snapshot = await parseUnifiedDiffAsync(response.unifiedDiff, "workspace", response.repositoryRoot);
  return diffFileFromParsedWorkspaceDetail(response, fallbackFile, snapshot, expansion);
}

// diffFileFromParsedWorkspaceDetail merges parsed file detail with the existing summary slot.
// diffFileFromParsedWorkspaceDetail 将已解析文件详情合并到已有摘要槽位。
function diffFileFromParsedWorkspaceDetail(
  response: WorkspaceDiffFileDetailResponse,
  fallbackFile: DiffFile,
  snapshot: DiffPanelSnapshot,
  requestedExpansion?: DiffHunkExpansionRequest,
): DiffFile | null {
  const summary = response.file ?? workspaceDiffSummaryFromFile(fallbackFile);
  const [file] = mergeWorkspaceDiffSummaries([summary], snapshot.files, Boolean(response.truncated));
  if (!file) {
    return null;
  }
  const expansion = normalizedWorkspaceDiffHunkExpansion(response, requestedExpansion);
  const mergedFile = expansion ? diffFileWithExpandedHunk(fallbackFile, file, expansion) ?? file : file;
  const hunkScopedSummaryDetail = Boolean(expansion && fallbackFile.isPartial && fallbackFile.hunks.length === 0);
  const estimatedRowCount = monotonicWorkspaceDiffCountEstimate(
    fallbackFile.estimatedRowCount,
    mergedFile.estimatedRowCount,
    mergedFile.rowCount,
  );
  const estimatedHunkCount = monotonicWorkspaceDiffCountEstimate(
    fallbackFile.estimatedHunkCount,
    mergedFile.estimatedHunkCount,
    mergedFile.hunks.length,
  );
  return {
    ...mergedFile,
    key: fallbackFile.key,
    estimatedRowCount,
    estimatedHunkCount,
    isPartial: hunkScopedSummaryDetail || Boolean(response.truncated),
    fileDiffLoadState: hunkScopedSummaryDetail
      ? { status: "loaded", message: response.notice ?? "已加载搜索命中的 hunk，完整文件内容仍可按需加载。" }
      : response.truncated
      ? { status: "loaded", message: response.notice ?? "文件 diff 详情已加载，但内容仍被截断。" }
      : { status: "loaded", message: response.notice ?? null },
    rawDiff: response.unifiedDiff || file.rawDiff || fallbackFile.rawDiff,
    contextLineCount: response.contextLines ?? fallbackFile.contextLineCount,
  };
}

// monotonicWorkspaceDiffCountEstimate returns a non-decreasing row or hunk estimate.
// monotonicWorkspaceDiffCountEstimate 返回不会减少的行数或 hunk 数估算。
function monotonicWorkspaceDiffCountEstimate(
  // Existing estimate from the previous summary or hydrated file.
  // 来自上一版摘要或已水合文件的既有估算。
  previousEstimate: number | null | undefined,
  // New estimate returned with the latest detail payload.
  // 最新详情载荷返回的新估算。
  nextEstimate: number | null | undefined,
  // Parsed concrete count from the latest detail payload.
  // 最新详情载荷解析出的实际数量。
  parsedCount: number | null | undefined,
): number | undefined {
  const value = Math.max(previousEstimate ?? 0, nextEstimate ?? 0, parsedCount ?? 0);
  return value > 0 ? value : undefined;
}

// normalizedWorkspaceDiffHunkExpansion resolves the requested or echoed hunk expansion intent.
// normalizedWorkspaceDiffHunkExpansion 解析请求或响应回传的 hunk 展开意图。
function normalizedWorkspaceDiffHunkExpansion(
  response: WorkspaceDiffFileDetailResponse,
  requestedExpansion: DiffHunkExpansionRequest | undefined,
): DiffHunkExpansionRequest | null {
  const hunkIndex = requestedExpansion?.hunkIndex ?? response.hunkIndex;
  const direction = requestedExpansion?.direction ?? response.hunkExpansionDirection ?? null;
  if (typeof hunkIndex !== "number" || !Number.isInteger(hunkIndex) || hunkIndex < 0 || !isDiffHunkExpansionDirection(direction)) {
    return null;
  }
  return { hunkIndex, direction };
}

// diffFileWithExpandedHunk merges one targeted expanded hunk into the currently rendered file.
// diffFileWithExpandedHunk 将单个定向展开 hunk 合并进当前已渲染文件。
function diffFileWithExpandedHunk(
  fallbackFile: DiffFile,
  detailFile: DiffFile,
  expansion: DiffHunkExpansionRequest,
): DiffFile | null {
  const originalHunk = fallbackFile.hunks[expansion.hunkIndex];
  if (!originalHunk || !fallbackFile.hunks.length || !detailFile.hunks.length) {
    return null;
  }
  const expandedHunk = findExpandedHunkForOriginalHunk(detailFile.hunks, originalHunk);
  if (!expandedHunk) {
    return null;
  }
  const clippedHunk = clippedExpandedHunkForDirection(expandedHunk, originalHunk, expansion.direction);
  const affectedRange = affectedOriginalHunkRange(fallbackFile.hunks, clippedHunk, expansion.hunkIndex);
  if (!affectedRange) {
    return null;
  }
  const hunks = diffHunksWithReplacement(fallbackFile.hunks, affectedRange.start, affectedRange.end, clippedHunk);
  return {
    ...detailFile,
    hunks,
    rowCount: diffHunksRowCount(hunks),
  };
}

// findExpandedHunkForOriginalHunk finds the larger Git hunk that contains the current hunk anchors.
// findExpandedHunkForOriginalHunk 查找包含当前 hunk 锚点的更大 Git hunk。
function findExpandedHunkForOriginalHunk(expandedHunks: readonly DiffHunk[], originalHunk: DiffHunk): DiffHunk | null {
  const anchors = diffHunkAnchors(originalHunk);
  if (!anchors) {
    return null;
  }
  return expandedHunks.find((hunk) => diffHunkContainsAnchors(hunk, anchors)) ?? null;
}

// clippedExpandedHunkForDirection trims a larger hunk to the requested expansion side.
// clippedExpandedHunkForDirection 按请求方向裁剪更大的 hunk。
function clippedExpandedHunkForDirection(
  expandedHunk: DiffHunk,
  originalHunk: DiffHunk,
  direction: DiffHunkExpansionDirection,
): DiffHunk {
  if (direction === "both") {
    return expandedHunk;
  }
  const anchors = diffHunkAnchors(originalHunk);
  if (!anchors) {
    return expandedHunk;
  }
  const firstIndex = diffHunkLineIdentityIndex(expandedHunk, anchors.first);
  const lastIndex = diffHunkLineIdentityIndex(expandedHunk, anchors.last);
  if (firstIndex < 0 || lastIndex < 0 || firstIndex > lastIndex) {
    return expandedHunk;
  }
  const lines = direction === "up"
    ? copiedDiffLines(expandedHunk.lines, 0, lastIndex + 1)
    : copiedDiffLines(expandedHunk.lines, firstIndex, expandedHunk.lines.length);
  return diffHunkWithLines(expandedHunk, lines);
}

// diffHunksWithReplacement returns one hunk list with a contiguous range replaced.
// diffHunksWithReplacement 返回替换连续范围后的 hunk 列表。
function diffHunksWithReplacement(
  hunks: readonly DiffHunk[],
  start: number,
  end: number,
  replacement: DiffHunk,
): DiffHunk[] {
  const replacementCount = Math.max(0, end - start + 1);
  const nextHunks: DiffHunk[] = new Array(hunks.length - replacementCount + 1);
  let nextIndex = 0;
  for (let index = 0; index < start; index += 1) {
    nextHunks[nextIndex] = hunks[index];
    nextIndex += 1;
  }
  nextHunks[nextIndex] = replacement;
  nextIndex += 1;
  for (let index = end + 1; index < hunks.length; index += 1) {
    nextHunks[nextIndex] = hunks[index];
    nextIndex += 1;
  }
  return nextHunks;
}

// copiedDiffLines copies a bounded line range without slice helper allocation paths.
// copiedDiffLines 复制受限行范围，避免通过 slice 派生。
function copiedDiffLines(lines: readonly DiffLine[], start: number, end: number): DiffLine[] {
  const safeStart = Math.max(0, Math.min(start, lines.length));
  const safeEnd = Math.max(safeStart, Math.min(end, lines.length));
  const copied: DiffLine[] = new Array(safeEnd - safeStart);
  for (let index = safeStart; index < safeEnd; index += 1) {
    copied[index - safeStart] = lines[index];
  }
  return copied;
}

// affectedOriginalHunkRange returns the original hunk span replaced by the clipped hunk.
// affectedOriginalHunkRange 返回被裁剪 hunk 替换的原始 hunk 范围。
function affectedOriginalHunkRange(
  originalHunks: readonly DiffHunk[],
  clippedHunk: DiffHunk,
  fallbackIndex: number,
): { start: number; end: number } | null {
  let start = Number.POSITIVE_INFINITY;
  let end = Number.NEGATIVE_INFINITY;
  let includesFallback = false;
  for (let index = 0; index < originalHunks.length; index += 1) {
    if (!diffHunkContainsOriginalHunk(clippedHunk, originalHunks[index])) {
      continue;
    }
    start = Math.min(start, index);
    end = Math.max(end, index);
    if (index === fallbackIndex) {
      includesFallback = true;
    }
  }
  if (!Number.isFinite(start)) {
    start = fallbackIndex;
    end = fallbackIndex;
  } else if (!includesFallback) {
    start = Math.min(start, fallbackIndex);
    end = Math.max(end, fallbackIndex);
  }
  return { start, end };
}

// diffHunkContainsOriginalHunk returns whether a rendered hunk contains another hunk's anchors.
// diffHunkContainsOriginalHunk 返回一个已渲染 hunk 是否包含另一个 hunk 的锚点。
function diffHunkContainsOriginalHunk(container: DiffHunk, original: DiffHunk): boolean {
  const anchors = diffHunkAnchors(original);
  return Boolean(anchors && diffHunkContainsAnchors(container, anchors));
}

// diffHunkContainsAnchors returns whether both anchor identities are present in one hunk.
// diffHunkContainsAnchors 返回一个 hunk 是否同时包含两端锚点身份。
function diffHunkContainsAnchors(hunk: DiffHunk, anchors: DiffHunkAnchors): boolean {
  let hasFirst = false;
  let hasLast = false;
  for (const line of hunk.lines) {
    const identity = diffLineIdentity(line);
    hasFirst = hasFirst || identity === anchors.first;
    hasLast = hasLast || identity === anchors.last;
    if (hasFirst && hasLast) {
      return true;
    }
  }
  return false;
}

// diffHunkLineIdentityIndex finds one anchor inside a parsed hunk.
// diffHunkLineIdentityIndex 在已解析 hunk 中查找单个锚点。
function diffHunkLineIdentityIndex(hunk: DiffHunk, identity: string): number {
  return hunk.lines.findIndex((line) => diffLineIdentity(line) === identity);
}

// DiffHunkAnchors stores stable line identities for matching expanded hunks.
// DiffHunkAnchors 保存用于匹配扩展 hunk 的稳定行身份。
interface DiffHunkAnchors {
  // First meaningful line identity in the original hunk.
  // 原始 hunk 中第一条有效行身份。
  first: string;
  // Last meaningful line identity in the original hunk.
  // 原始 hunk 中最后一条有效行身份。
  last: string;
}

// diffHunkAnchors extracts stable first and last line identities from one hunk.
// diffHunkAnchors 从单个 hunk 提取稳定的首尾行身份。
function diffHunkAnchors(hunk: DiffHunk): DiffHunkAnchors | null {
  let first: DiffLine | null = null;
  let last: DiffLine | null = null;
  for (const line of hunk.lines) {
    if (line.oldLine == null && line.newLine == null) {
      continue;
    }
    first ??= line;
    last = line;
  }
  if (!first || !last) {
    return null;
  }
  return {
    first: diffLineIdentity(first),
    last: diffLineIdentity(last),
  };
}

// diffLineIdentity builds a stable identity from line numbers, kind, and content.
// diffLineIdentity 根据行号、类型与内容构建稳定行身份。
function diffLineIdentity(line: DiffLine): string {
  return [
    line.kind,
    line.oldLine ?? "",
    line.newLine ?? "",
    line.content,
  ].join("\u0000");
}

// diffHunkWithLines rebuilds hunk metadata after directional clipping.
// diffHunkWithLines 在方向裁剪后重建 hunk 元数据。
function diffHunkWithLines(source: DiffHunk, lines: DiffLine[]): DiffHunk {
  const oldStart = firstNumberedLine(lines, "oldLine") ?? source.oldStart;
  const newStart = firstNumberedLine(lines, "newLine") ?? source.newStart;
  return {
    ...source,
    header: diffHunkHeader(source.header, oldStart, diffHunkOldLineCount(lines), newStart, diffHunkNewLineCount(lines)),
    oldStart,
    newStart,
    lines,
  };
}

// firstNumberedLine returns the first available old or new line number.
// firstNumberedLine 返回首个可用的旧文件或新文件行号。
function firstNumberedLine(lines: readonly DiffLine[], key: "oldLine" | "newLine"): number | null {
  for (const line of lines) {
    const lineNumber = line[key];
    if (lineNumber != null) {
      return lineNumber;
    }
  }
  return null;
}

// diffHunkOldLineCount counts lines represented on the old side.
// diffHunkOldLineCount 统计旧文件侧覆盖的行数。
function diffHunkOldLineCount(lines: readonly DiffLine[]): number {
  let count = 0;
  for (const line of lines) {
    if (line.oldLine != null) {
      count += 1;
    }
  }
  return count;
}

// diffHunkNewLineCount counts lines represented on the new side.
// diffHunkNewLineCount 统计新文件侧覆盖的行数。
function diffHunkNewLineCount(lines: readonly DiffLine[]): number {
  let count = 0;
  for (const line of lines) {
    if (line.newLine != null) {
      count += 1;
    }
  }
  return count;
}

// diffHunkHeader rebuilds the unified-diff hunk header and preserves any trailing function label.
// diffHunkHeader 重建 unified-diff hunk 头部，并保留尾部函数标签。
function diffHunkHeader(sourceHeader: string, oldStart: number, oldCount: number, newStart: number, newCount: number): string {
  const suffix = sourceHeader.match(/^@@\s+-\d+(?:,\d+)?\s+\+\d+(?:,\d+)?\s+@@(.*)$/)?.[1] ?? "";
  return `@@ -${oldStart},${oldCount} +${newStart},${newCount} @@${suffix}`;
}

// diffHunksRowCount counts visual rows including hunk headers.
// diffHunksRowCount 统计包含 hunk 头部的可视行数。
function diffHunksRowCount(hunks: readonly DiffHunk[]): number {
  let total = 0;
  for (const hunk of hunks) {
    total += 1 + hunk.lines.length;
  }
  return total;
}

// isDiffHunkExpansionDirection validates a nullable hunk direction string.
// isDiffHunkExpansionDirection 校验可空的 hunk 方向字符串。
function isDiffHunkExpansionDirection(value: string | null | undefined): value is DiffHunkExpansionDirection {
  return value === "up" || value === "down" || value === "both";
}

// diffSnapshotWithWorkspaceDetailFile merges one already parsed file detail into an existing snapshot.
// diffSnapshotWithWorkspaceDetailFile 将一个已解析文件详情合并进现有快照。
export function diffSnapshotWithWorkspaceDetailFile(
  previous: DiffPanelSnapshot,
  detailFile: DiffFile | null,
  fallbackFile: DiffFile,
): DiffPanelSnapshot | null {
  if (!detailFile) {
    return null;
  }
  const fileIndex = previous.files.findIndex((item) => item.key === fallbackFile.key);
  if (fileIndex < 0) {
    return null;
  }
  const files = [...previous.files];
  files[fileIndex] = detailFile;
  return {
    ...previous,
    files,
    rawDiff: loadedRawDiffFromFiles(files, previous.rawDiff),
    completeness: files.some((file) => file.isPartial) ? "partial" : previous.completeness,
  };
}

// mergeWorkspaceDiffSummaries keeps complete backend file navigation while reusing parsed preview hunks.
// mergeWorkspaceDiffSummaries 保留完整后端文件导航，同时复用已解析预览 hunk。
function mergeWorkspaceDiffSummaries(
  summaries: WorkspaceDiffFileSummary[],
  parsedFiles: DiffFile[],
  truncated: boolean,
): DiffFile[] {
  const parsedByPath = new Map<string, DiffFile>();
  for (const file of parsedFiles) {
    parsedByPath.set(workspaceDiffFilePathKey(file.oldPath, file.newPath), file);
  }
  const files: DiffFile[] = new Array(summaries.length);
  for (let index = 0; index < summaries.length; index += 1) {
    const summary = summaries[index];
    const parsedFile = parsedByPath.get(workspaceDiffFilePathKey(summary.oldPath, summary.newPath));
    const largeReasons = diffFileLargeReasons(summary);
    files[index] = {
      key: summary.key,
      oldPath: summary.oldPath,
      newPath: summary.newPath,
      status: summary.status,
      additions: summary.additions,
      deletions: summary.deletions,
      isBinary: summary.isBinary,
      isGitlink: summary.isGitlink,
      fileMode: summary.fileMode ?? null,
      changedLines: summary.changedLines,
      changedBytes: summary.changedBytes,
      maxChangedLineBytes: summary.maxChangedLineBytes,
      isLarge: largeReasons.length > 0,
      tooLargeToDisplay: Boolean(summary.tooLargeToDisplay),
      largeReasons,
      rowCount: parsedFile?.rowCount,
      estimatedRowCount: summary.rowCount,
      estimatedHunkCount: summary.hunkCount,
      oldFileLineCount: summary.oldFileLineCount ?? parsedFile?.oldFileLineCount ?? null,
      newFileLineCount: summary.newFileLineCount ?? parsedFile?.newFileLineCount ?? null,
      isPartial: (truncated || summary.tooLargeToDisplay) && !summary.isBinary && !summary.isGitlink && !parsedFile?.hunks.length,
      fileDiffLoadState: parsedFile?.fileDiffLoadState
        ?? (summary.tooLargeToDisplay && !summary.isBinary && !summary.isGitlink && !parsedFile?.hunks.length
          ? { status: "loaded", message: "该文件超过 WebUI 展示阈值，未传输 diff 正文。" }
          : truncated && !summary.isBinary && !summary.isGitlink && !parsedFile?.hunks.length
            ? { status: "idle" }
          : parsedFile?.hunks.length
            ? { status: "loaded" }
            : undefined),
      contextLineCount: parsedFile?.contextLineCount,
      rawDiff: parsedFile?.rawDiff,
      hunks: parsedFile?.hunks ?? [],
    };
  }
  return files;
}

// workspaceDiffFileFieldsFromSummary maps backend summary fields onto an existing file detail.
// workspaceDiffFileFieldsFromSummary 将后端摘要字段映射到已有文件详情上。
function workspaceDiffFileFieldsFromSummary(
  // Backend file summary returned with a skipped or loaded detail response.
  // 跳过或已加载详情响应中返回的后端文件摘要。
  summary: WorkspaceDiffFileSummary,
): Pick<DiffFile, "oldPath" | "newPath" | "status" | "additions" | "deletions" | "isBinary" | "isGitlink" | "fileMode" | "changedLines" | "changedBytes" | "maxChangedLineBytes" | "isLarge" | "tooLargeToDisplay" | "largeReasons" | "estimatedRowCount" | "estimatedHunkCount" | "oldFileLineCount" | "newFileLineCount"> {
  const largeReasons = diffFileLargeReasons(summary);
  return {
    oldPath: summary.oldPath,
    newPath: summary.newPath,
    status: summary.status,
    additions: summary.additions,
    deletions: summary.deletions,
    isBinary: summary.isBinary,
    isGitlink: summary.isGitlink,
    fileMode: summary.fileMode ?? null,
    changedLines: summary.changedLines,
    changedBytes: summary.changedBytes,
    maxChangedLineBytes: summary.maxChangedLineBytes,
    isLarge: largeReasons.length > 0,
    tooLargeToDisplay: Boolean(summary.tooLargeToDisplay),
    largeReasons,
    estimatedRowCount: summary.rowCount,
    estimatedHunkCount: summary.hunkCount,
    oldFileLineCount: summary.oldFileLineCount ?? null,
    newFileLineCount: summary.newFileLineCount ?? null,
  };
}

// loadedRawDiffFromFiles rebuilds copyable raw diff from loaded file sections.
// loadedRawDiffFromFiles 根据已加载文件片段重建可复制的 raw diff。
function loadedRawDiffFromFiles(files: DiffFile[], fallbackRawDiff: string): string {
  const sections: string[] = [];
  for (const file of files) {
    const section = normalizeRawDiffSection(file.rawDiff);
    if (section) {
      sections.push(section);
    }
  }
  if (!sections.length) {
    return fallbackRawDiff;
  }
  return sections.join("\n");
}

// normalizeRawDiffSection trims only redundant outer blank space from one file patch.
// normalizeRawDiffSection 仅裁剪单个文件 patch 外层冗余空白。
function normalizeRawDiffSection(rawDiff: string | null | undefined): string | null {
  const normalized = rawDiff?.replace(/\r\n/g, "\n").trim();
  return normalized ? normalized : null;
}

// workspaceDiffSummaryFromFile projects a rendered file back into backend-summary shape.
// workspaceDiffSummaryFromFile 将已渲染文件投影回后端摘要形态。
function workspaceDiffSummaryFromFile(file: DiffFile): WorkspaceDiffFileSummary {
  return {
    key: file.key,
    oldPath: file.oldPath,
    newPath: file.newPath,
    status: file.status,
    additions: file.additions,
    deletions: file.deletions,
    isBinary: file.isBinary,
    isGitlink: file.isGitlink,
    fileMode: file.fileMode,
    changedLines: file.changedLines,
    tooLargeToDisplay: file.tooLargeToDisplay,
    changedBytes: file.changedBytes,
    maxChangedLineBytes: file.maxChangedLineBytes,
    rowCount: file.estimatedRowCount ?? file.rowCount ?? 0,
    oldFileLineCount: file.oldFileLineCount ?? null,
    newFileLineCount: file.newFileLineCount ?? null,
    hunkCount: file.estimatedHunkCount ?? file.hunks.length,
  };
}

// workspaceDiffFilePathKey builds a stable merge key for backend summaries and parsed preview files.
// workspaceDiffFilePathKey 为后端摘要与已解析预览文件构建稳定合并键。
function workspaceDiffFilePathKey(oldPath: string, newPath: string): string {
  return `${oldPath}\u0000${newPath}`;
}

// diffReviewSelectionStableKey builds a compact stable key for one Git review selection.
// diffReviewSelectionStableKey 为单个 Git 审核选择构建紧凑稳定键。
export function diffReviewSelectionStableKey(selection: DiffReviewSelection): string {
  return [
    selection.mode,
    selection.baseBranch?.trim() ?? "",
    selection.compareBranch?.trim() ?? "",
    selection.commitHash?.trim() ?? "",
  ].join("\u001f");
}

// latestConversationDiffSource resolves the newest raw diff from the selected conversation cache.
// latestConversationDiffSource 从当前选中会话缓存中解析最新的原始 diff 来源。
export function latestConversationDiffSource(
  workbench: WorkbenchSnapshot,
  selectedConversationId: string | null | undefined,
  parseBlocks: (message: ConversationMessage) => WorkspaceReviewConversationBlock[],
): WorkspaceReviewConversationDiffSource | null {
  const activeSurfaceId = activeWorkbenchSurfaceId(workbench, selectedConversationId);
  if (!activeSurfaceId) {
    return null;
  }
  for (let messageIndex = workbench.messages.length - 1; messageIndex >= 0; messageIndex -= 1) {
    const message = workbench.messages[messageIndex];
    if (message.conversationId !== activeSurfaceId) {
      continue;
    }
    const blocks = parseBlocks(message);
    for (let blockIndex = blocks.length - 1; blockIndex >= 0; blockIndex -= 1) {
      const block = blocks[blockIndex];
      if (block.kind === "diff" && hasRenderableDiff(block.body)) {
        return {
          key: `${activeSurfaceId}\u0000${message.id}\u0000${blockIndex}\u0000${block.body.length}`,
          rawDiff: block.body,
          source: diffPanelSourceForMessage(message),
        };
      }
    }
  }
  return null;
}

// diffPanelSourceForMessage derives a display source from structured message metadata.
// diffPanelSourceForMessage 从结构化消息元数据中推导展示来源。
function diffPanelSourceForMessage(message: ConversationMessage): DiffPanelSnapshot["source"] {
  const hostKind = metadataString(message.metadata, ["kind", "host_result.kind", "hostResult.kind"]);
  if (hostKind === "change_set") {
    return "change_set";
  }
  return "message";
}

// metadataString reads one nested metadata string by candidate paths.
// metadataString 按候选路径读取单个嵌套元数据字符串。
function metadataString(metadata: unknown, paths: string[]): string | null {
  for (const path of paths) {
    const value = metadataValue(metadata, path.split("."));
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

// metadataValue reads one nested metadata value by path parts.
// metadataValue 按路径片段读取单个嵌套元数据值。
function metadataValue(metadata: unknown, paths: string[]): unknown {
  let current = metadata;
  for (const path of paths) {
    if (!current || typeof current !== "object" || !(path in current)) {
      return null;
    }
    current = (current as Record<string, unknown>)[path];
  }
  return current;
}
