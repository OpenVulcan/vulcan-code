import type {
  DiffReviewSelection,
  WorkspaceDiffFileDetailRequest,
  WorkspaceDiffFileDetailResponse,
  WorkspaceDiffFileSummary,
  WorkspaceDiffSearchRequest,
  WorkspaceDiffSearchResponse,
  WorkspaceDiffSnapshotResponse,
} from "@/diff/types";

// BrowserFallbackDiffFileRecord stores one generated preview diff file and its backend-style summary.
// BrowserFallbackDiffFileRecord 保存一个生成的预览 diff 文件及其后端摘要。
interface BrowserFallbackDiffFileRecord {
  // Backend merge key for this generated file.
  // 该生成文件的后端合并键。
  key: string;
  // Summary metadata exposed by the snapshot request before full file hydration.
  // 快照请求在完整文件水合前暴露的摘要元数据。
  summary: WorkspaceDiffFileSummary;
  // Complete unified diff section returned by file-detail requests.
  // 文件详情请求返回的完整 unified diff 片段。
  unifiedDiff: string;
}

// BrowserFallbackDiffStore stores generated review-diff payloads reused across preview calls.
// BrowserFallbackDiffStore 保存跨预览调用复用的生成审核 diff 载荷。
interface BrowserFallbackDiffStore {
  // Full generated file records in diff order.
  // 按 diff 顺序排列的完整生成文件记录。
  records: BrowserFallbackDiffFileRecord[];
  // Cached snapshot summaries returned directly by the snapshot endpoint.
  // 快照端点直接返回的缓存摘要列表。
  snapshotFiles: WorkspaceDiffFileSummary[];
  // Initial truncated unified diff sent by the snapshot endpoint.
  // 快照端点返回的初始截断 unified diff。
  previewUnifiedDiff: string;
  // Aggregate additions across the complete generated diff.
  // 完整生成 diff 的新增行总数。
  additions: number;
  // Aggregate deletions across the complete generated diff.
  // 完整生成 diff 的删除行总数。
  deletions: number;
  // Aggregate changed line count across the complete generated diff.
  // 完整生成 diff 的变更行总数。
  totalChangedLines: number;
  // Aggregate changed byte estimate across the complete generated diff.
  // 完整生成 diff 的变更字节估算总数。
  totalChangedBytes: number;
}

// BrowserFallbackDiffProfile describes one mode-sensitive preview diff flavor.
// BrowserFallbackDiffProfile 描述一个随审核模式变化的预览 diff 形态。
interface BrowserFallbackDiffProfile {
  // Stable store key used to cache this generated diff.
  // 用于缓存该生成 diff 的稳定 store key。
  key: string;
  // Path segment embedded in generated file names.
  // 嵌入生成文件名中的路径片段。
  pathSegment: string;
  // Stable marker embedded in changed lines for browser verification.
  // 嵌入变更行中用于浏览器验证的稳定标记。
  marker: string;
  // Notice label shown with the generated snapshot.
  // 生成快照中展示的提示标签。
  label: string;
  // Selected base branch shown by the toolbar.
  // 工具栏展示的选中基准分支。
  selectedBaseBranch: string | null;
  // Selected compare branch shown by the toolbar.
  // 工具栏展示的选中目标分支。
  selectedCompareBranch: string | null;
  // Selected commit hash shown by the toolbar.
  // 工具栏展示的选中提交哈希。
  selectedCommitHash: string | null;
}

// BROWSER_FALLBACK_DIFF_FILE_COUNT keeps browser preview above the target file-count gate.
// BROWSER_FALLBACK_DIFF_FILE_COUNT 让浏览器预览超过目标文件数量阈值。
const BROWSER_FALLBACK_DIFF_FILE_COUNT = 156;

// BROWSER_FALLBACK_PREVIEW_FILE_COUNT limits initial parsing to a small target-style preview.
// BROWSER_FALLBACK_PREVIEW_FILE_COUNT 将初始解析限制为少量目标式预览文件。
const BROWSER_FALLBACK_PREVIEW_FILE_COUNT = 8;

// BROWSER_FALLBACK_LARGE_FILE_INDEX marks the generated file used for row virtualization checks.
// BROWSER_FALLBACK_LARGE_FILE_INDEX 标记用于行虚拟化检查的生成大文件。
const BROWSER_FALLBACK_LARGE_FILE_INDEX = 28;

// BROWSER_FALLBACK_SEARCH_MATCH_LIMIT bounds preview backend search results.
// BROWSER_FALLBACK_SEARCH_MATCH_LIMIT 限制预览后端搜索结果数量。
const BROWSER_FALLBACK_SEARCH_MATCH_LIMIT = 200;

// browserFallbackDiffProfileForSelection resolves deterministic preview data for one review selection.
// browserFallbackDiffProfileForSelection 为单个审核选择解析确定性的预览数据。
function browserFallbackDiffProfileForSelection(selection: DiffReviewSelection): BrowserFallbackDiffProfile {
  if (selection.mode === "staged") {
    return {
      key: "staged",
      pathSegment: "staged-index",
      marker: "staged-index",
      label: "staged",
      selectedBaseBranch: null,
      selectedCompareBranch: null,
      selectedCommitHash: null,
    };
  }
  if (selection.mode === "commit") {
    const commitHash = selection.commitHash?.trim() || "f00dbabe";
    const shortHash = commitHash.slice(0, 8) || "f00dbabe";
    return {
      key: `commit:${commitHash}`,
      pathSegment: `commit-${browserFallbackSafePathSegment(shortHash)}`,
      marker: `commit-${browserFallbackSafePathSegment(shortHash)}`,
      label: "commit",
      selectedBaseBranch: null,
      selectedCompareBranch: null,
      selectedCommitHash: commitHash,
    };
  }
  if (selection.mode === "branch") {
    const baseBranch = selection.baseBranch?.trim() || "main";
    const compareBranch = selection.compareBranch?.trim() || "codex/webui-diff-preview";
    return {
      key: `branch:${baseBranch}->${compareBranch}`,
      pathSegment: `branch-${browserFallbackSafePathSegment(baseBranch)}-to-${browserFallbackSafePathSegment(compareBranch)}`,
      marker: `branch-${browserFallbackSafePathSegment(baseBranch)}-to-${browserFallbackSafePathSegment(compareBranch)}`,
      label: "branch",
      selectedBaseBranch: baseBranch,
      selectedCompareBranch: compareBranch,
      selectedCommitHash: null,
    };
  }
  return {
    key: "unstaged",
    pathSegment: "unstaged-working-tree",
    marker: "unstaged-working-tree",
    label: "unstaged",
    selectedBaseBranch: null,
    selectedCompareBranch: null,
    selectedCommitHash: null,
  };
}

// browserFallbackSafePathSegment converts a Git ref or hash into a generated path segment.
// browserFallbackSafePathSegment 将 Git 引用或哈希转换为生成路径片段。
function browserFallbackSafePathSegment(value: string): string {
  const normalized = value.trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
  return normalized || "default";
}

// browserFallbackDiffStores cache generated diff payloads per review selection.
// browserFallbackDiffStores 按审核选择缓存生成 diff 载荷。
const browserFallbackDiffStores = new Map<string, BrowserFallbackDiffStore>();

// loadBrowserFallbackWorkspaceDiffSnapshot returns a target-shaped large diff preview without Core Service.
// loadBrowserFallbackWorkspaceDiffSnapshot 在没有 Core Service 时返回目标形态的大 diff 预览。
export async function loadBrowserFallbackWorkspaceDiffSnapshot(
  workspacePath: string,
  request: DiffReviewSelection,
): Promise<WorkspaceDiffSnapshotResponse> {
  const profile = browserFallbackDiffProfileForSelection(request);
  const store = browserFallbackDiffStoreForPreview(profile);
  return {
    gitRepository: true,
    repositoryRoot: workspacePath,
    unifiedDiff: store.previewUnifiedDiff,
    changedFiles: store.records.length,
    files: store.snapshotFiles,
    additions: store.additions,
    deletions: store.deletions,
    totalChangedLines: store.totalChangedLines,
    totalChangedBytes: store.totalChangedBytes,
    truncated: true,
    notice: `Browser preview mode is using a target-shaped large ${profile.label} diff sample: summaries first, file details on demand.`,
    currentBranch: "codex/webui-diff-preview",
    defaultBranch: "main",
    selectedBaseBranch: profile.selectedBaseBranch,
    selectedCompareBranch: profile.selectedCompareBranch,
    selectedCommitHash: profile.selectedCommitHash,
    branches: [
      { name: "main", current: false, remote: false },
      { name: "codex/webui-diff-preview", current: true, remote: false },
      { name: "feature/large-review", current: false, remote: false },
    ],
    recentCommits: [
      { hash: "f00dbabe", shortHash: "f00dbab", subject: "Preview high-performance WebUI diff rendering", committedAtMs: Date.now() },
      { hash: "c0ffee42", shortHash: "c0ffee4", subject: "Complete summary/detail review data", committedAtMs: Date.now() - 3_600_000 },
    ],
  };
}

// loadBrowserFallbackWorkspaceDiffFileDetail returns the full generated diff for one summary file.
// loadBrowserFallbackWorkspaceDiffFileDetail 返回单个摘要文件对应的完整生成 diff。
export async function loadBrowserFallbackWorkspaceDiffFileDetail(
  workspacePath: string,
  request: DiffReviewSelection,
  file: WorkspaceDiffFileDetailRequest,
  options?: { signal?: AbortSignal },
): Promise<WorkspaceDiffFileDetailResponse> {
  throwIfBrowserFallbackAborted(options?.signal);
  const record = browserFallbackDiffRecordForRequest(request, file);
  return {
    repositoryRoot: workspacePath,
    file: record?.summary ?? null,
    unifiedDiff: record?.unifiedDiff ?? "",
    truncated: false,
    notice: record ? "Browser preview mode loaded the file diff detail on demand." : "Browser preview mode could not find the requested file diff.",
    contextLines: file.contextLines ?? null,
    hunkIndex: file.hunkIndex ?? null,
    hunkExpansionDirection: file.hunkExpansionDirection ?? null,
  };
}

// loadBrowserFallbackWorkspaceDiffSearch scans the generated complete diff and returns compact file hits.
// loadBrowserFallbackWorkspaceDiffSearch 扫描生成的完整 diff 并返回紧凑文件命中。
export async function loadBrowserFallbackWorkspaceDiffSearch(
  workspacePath: string,
  request: DiffReviewSelection,
  search: WorkspaceDiffSearchRequest,
): Promise<WorkspaceDiffSearchResponse> {
  const query = search.query.trim();
  const store = browserFallbackDiffStoreForPreview(browserFallbackDiffProfileForSelection(request));
  if (!query) {
    return {
      repositoryRoot: workspacePath,
      query,
      matches: [],
      truncated: false,
      scannedFiles: store.records.length,
      scannedLines: 0,
    };
  }
  const limit = Math.max(1, Math.min(search.limit ?? BROWSER_FALLBACK_SEARCH_MATCH_LIMIT, BROWSER_FALLBACK_SEARCH_MATCH_LIMIT));
  const normalizedQuery = query.toLowerCase();
  let scannedLines = 0;
  const matches: WorkspaceDiffSearchResponse["matches"] = [];
  for (const record of store.records) {
    const match = browserFallbackDiffSearchMatch(record, normalizedQuery);
    scannedLines += match.scannedLines;
    if (!match.preview) {
      continue;
    }
    matches.push({
      key: record.summary.key,
      oldPath: record.summary.oldPath,
      newPath: record.summary.newPath,
      status: record.summary.status,
      matchCount: match.matchCount,
      firstPreview: match.preview,
      previews: match.previews,
      firstHunkIndex: match.hunkIndex,
      firstOldLine: match.oldLine,
      firstNewLine: match.newLine,
      firstLineKind: match.lineKind,
    });
    if (matches.length >= limit) {
      return {
        repositoryRoot: workspacePath,
        query,
        matches,
        truncated: true,
        scannedFiles: store.records.length,
        scannedLines,
        notice: "Browser preview search results were truncated at the limit.",
      };
    }
  }
  return {
    repositoryRoot: workspacePath,
    query,
    matches,
    truncated: false,
    scannedFiles: store.records.length,
    scannedLines,
  };
}

// browserFallbackDiffStoreForPreview returns the cached generated store or builds it once.
// browserFallbackDiffStoreForPreview 返回缓存的生成载荷，或首次构建它。
function browserFallbackDiffStoreForPreview(profile: BrowserFallbackDiffProfile): BrowserFallbackDiffStore {
  const cachedStore = browserFallbackDiffStores.get(profile.key);
  if (cachedStore) {
    return cachedStore;
  }
  const records: BrowserFallbackDiffFileRecord[] = new Array(BROWSER_FALLBACK_DIFF_FILE_COUNT);
  for (let index = 0; index < BROWSER_FALLBACK_DIFF_FILE_COUNT; index += 1) {
    records[index] = index === BROWSER_FALLBACK_LARGE_FILE_INDEX
      ? browserFallbackLargeDiffFileRecord(index, profile)
      : browserFallbackRegularDiffFileRecord(index, profile);
  }
  const store = {
    records,
    snapshotFiles: browserFallbackSnapshotFiles(records),
    previewUnifiedDiff: browserFallbackPreviewUnifiedDiff(records),
    additions: sumBrowserFallbackDiffMetric(records, "additions"),
    deletions: sumBrowserFallbackDiffMetric(records, "deletions"),
    totalChangedLines: sumBrowserFallbackDiffMetric(records, "changedLines"),
    totalChangedBytes: sumBrowserFallbackDiffMetric(records, "changedBytes"),
  };
  browserFallbackDiffStores.set(profile.key, store);
  return store;
}

// browserFallbackSnapshotFiles copies generated summaries once for repeated snapshot responses.
// browserFallbackSnapshotFiles 为重复快照响应一次性复制生成摘要。
function browserFallbackSnapshotFiles(records: readonly BrowserFallbackDiffFileRecord[]): WorkspaceDiffFileSummary[] {
  const summaries: WorkspaceDiffFileSummary[] = new Array(records.length);
  for (let index = 0; index < records.length; index += 1) {
    summaries[index] = records[index].summary;
  }
  return summaries;
}

// browserFallbackPreviewUnifiedDiff builds the target-style truncated preview without temporary arrays.
// browserFallbackPreviewUnifiedDiff 在不创建临时数组的情况下构建目标式截断预览。
function browserFallbackPreviewUnifiedDiff(records: readonly BrowserFallbackDiffFileRecord[]): string {
  let previewUnifiedDiff = "";
  let previewFileCount = 0;
  for (let index = 0; index < records.length && previewFileCount < BROWSER_FALLBACK_PREVIEW_FILE_COUNT; index += 1) {
    if (index === BROWSER_FALLBACK_LARGE_FILE_INDEX) {
      continue;
    }
    if (previewUnifiedDiff) {
      previewUnifiedDiff += "\n";
    }
    previewUnifiedDiff += records[index].unifiedDiff;
    previewFileCount += 1;
  }
  return previewUnifiedDiff;
}

// browserFallbackDiffRecordForRequest resolves a generated file record by stable key or path.
// browserFallbackDiffRecordForRequest 按稳定键或路径解析生成文件记录。
function browserFallbackDiffRecordForRequest(request: DiffReviewSelection, file: WorkspaceDiffFileDetailRequest): BrowserFallbackDiffFileRecord | null {
  const store = browserFallbackDiffStoreForPreview(browserFallbackDiffProfileForSelection(request));
  const requestedPath = file.newPath || file.oldPath;
  return store.records.find((record) => (
    record.key === file.key
    || record.summary.key === file.key
    || record.summary.newPath === requestedPath
    || record.summary.oldPath === requestedPath
  )) ?? null;
}

// browserFallbackLargeDiffFileRecord builds a file large enough to exercise row virtualization.
// browserFallbackLargeDiffFileRecord 构建足够触发行虚拟化的大文件。
function browserFallbackLargeDiffFileRecord(index: number, profile: BrowserFallbackDiffProfile): BrowserFallbackDiffFileRecord {
  const path = `apps/vulcan-code-web/src/diff/${profile.pathSegment}/generated-large-preview.tsx`;
  const lines: string[] = [
    `diff --git a/${path} b/${path}`,
    "index 91b5b3a..e40f70f 100644",
    `--- a/${path}`,
    `+++ b/${path}`,
    "@@ -1,2600 +1,2600 @@",
  ];
  for (let lineIndex = 0; lineIndex < 2600; lineIndex += 1) {
    lines.push(`-export const legacyPreviewRow${lineIndex} = "legacy-row-${lineIndex}";`);
    lines.push(`+export const optimizedPreviewRow${lineIndex} = "${profile.marker}-optimized-row-${lineIndex}";`);
  }
  return browserFallbackRecordFromLines(index, path, lines, 2600, 2600, 5201, 1);
}

// browserFallbackRegularDiffFileRecord builds a moderate TypeScript file diff for preview hydration.
// browserFallbackRegularDiffFileRecord 构建用于预览水合的中等 TypeScript 文件 diff。
function browserFallbackRegularDiffFileRecord(index: number, profile: BrowserFallbackDiffProfile): BrowserFallbackDiffFileRecord {
  const padded = String(index + 1).padStart(3, "0");
  const path = `apps/vulcan-code-web/src/preview/${profile.pathSegment}/generated-module-${padded}.ts`;
  const lines: string[] = [
    `diff --git a/${path} b/${path}`,
    "index 0a1b2c3..4d5e6f7 100644",
    `--- a/${path}`,
    `+++ b/${path}`,
  ];
  const hunkCount = 5;
  const changedPairsPerHunk = 8;
  let oldStart = index * 10 + 1;
  let newStart = index * 10 + 1;
  for (let hunkIndex = 0; hunkIndex < hunkCount; hunkIndex += 1) {
    lines.push(`@@ -${oldStart},18 +${newStart},18 @@ export function generatedModule${padded}Hunk${hunkIndex}()`);
    for (let contextIndex = 0; contextIndex < 2; contextIndex += 1) {
      lines.push(` const stableContext${contextIndex} = "module-${padded}-hunk-${hunkIndex}";`);
    }
    for (let pairIndex = 0; pairIndex < changedPairsPerHunk; pairIndex += 1) {
      const marker = `${padded}-${hunkIndex}-${pairIndex}`;
      lines.push(`-const legacyValue${pairIndex} = "legacy ${marker}";`);
      lines.push(`+const optimizedValue${pairIndex} = "${profile.marker} optimized ${marker} shiki target virtualization";`);
    }
    for (let contextIndex = 2; contextIndex < 4; contextIndex += 1) {
      lines.push(` return stableContext${contextIndex - 2} + " ${padded}";`);
    }
    oldStart += 22;
    newStart += 22;
  }
  return browserFallbackRecordFromLines(index, path, lines, hunkCount * changedPairsPerHunk, hunkCount * changedPairsPerHunk, hunkCount * 21, hunkCount);
}

// browserFallbackRecordFromLines converts generated unified-diff lines into one record and summary.
// browserFallbackRecordFromLines 将生成的 unified-diff 行转换为记录和摘要。
function browserFallbackRecordFromLines(
  index: number,
  path: string,
  lines: string[],
  additions: number,
  deletions: number,
  rowCount: number,
  hunkCount: number,
): BrowserFallbackDiffFileRecord {
  const unifiedDiff = lines.join("\n");
  const changedBytes = browserFallbackChangedBytes(lines);
  const summary: WorkspaceDiffFileSummary = {
    key: `${path}->${path}:${index}`,
    oldPath: path,
    newPath: path,
    status: "modified",
    additions,
    deletions,
    isBinary: false,
    isGitlink: false,
    fileMode: "100644",
    changedLines: additions + deletions,
    changedBytes,
    maxChangedLineBytes: browserFallbackMaxChangedLineBytes(lines),
    rowCount,
    hunkCount,
  };
  return { key: summary.key, summary, unifiedDiff };
}

// browserFallbackDiffSearchMatch scans one file diff and returns compact match metadata.
// browserFallbackDiffSearchMatch 扫描单个文件 diff 并返回紧凑命中元数据。
function browserFallbackDiffSearchMatch(
  record: BrowserFallbackDiffFileRecord,
  normalizedQuery: string,
): {
  preview: string | null;
  previews: string[];
  matchCount: number;
  scannedLines: number;
  hunkIndex: number | null;
  oldLine: number | null;
  newLine: number | null;
  lineKind: WorkspaceDiffSearchResponse["matches"][number]["firstLineKind"];
} {
  const lines = record.unifiedDiff.split("\n");
  const previews: string[] = [];
  let matchCount = 0;
  let hunkIndex = -1;
  let firstHunkIndex: number | null = null;
  let firstOldLine: number | null = null;
  let firstNewLine: number | null = null;
  let firstLineKind: WorkspaceDiffSearchResponse["matches"][number]["firstLineKind"] = null;
  for (let lineIndex = 0; lineIndex < lines.length; lineIndex += 1) {
    const line = lines[lineIndex];
    if (line.startsWith("@@ ")) {
      hunkIndex += 1;
      continue;
    }
    if (!line.toLowerCase().includes(normalizedQuery)) {
      continue;
    }
    matchCount += 1;
    if (previews.length < 4) {
      previews.push(line.trim().slice(0, 160));
    }
    if (firstHunkIndex === null) {
      firstHunkIndex = hunkIndex >= 0 ? hunkIndex : null;
      firstOldLine = browserFallbackApproximateLineNumber(lineIndex);
      firstNewLine = browserFallbackApproximateLineNumber(lineIndex);
      firstLineKind = browserFallbackLineKind(line);
    }
  }
  return {
    preview: previews[0] ?? null,
    previews,
    matchCount,
    scannedLines: lines.length,
    hunkIndex: firstHunkIndex,
    oldLine: firstOldLine,
    newLine: firstNewLine,
    lineKind: firstLineKind,
  };
}

// browserFallbackLineKind maps one unified-diff row prefix to a compact search kind.
// browserFallbackLineKind 将 unified-diff 行前缀映射为紧凑搜索类型。
function browserFallbackLineKind(line: string): WorkspaceDiffSearchResponse["matches"][number]["firstLineKind"] {
  if (line.startsWith("+") && !line.startsWith("+++")) {
    return "added";
  }
  if (line.startsWith("-") && !line.startsWith("---")) {
    return "removed";
  }
  if (line.startsWith("@@") || line.startsWith("diff --git") || line.startsWith("index ")) {
    return "metadata";
  }
  return "context";
}

// browserFallbackApproximateLineNumber returns a stable approximate line number for preview search.
// browserFallbackApproximateLineNumber 返回预览搜索使用的稳定近似行号。
function browserFallbackApproximateLineNumber(lineIndex: number): number {
  return Math.max(1, lineIndex - 3);
}

// browserFallbackChangedBytes estimates changed-line bytes in one generated diff.
// browserFallbackChangedBytes 估算单个生成 diff 中变更行字节数。
function browserFallbackChangedBytes(lines: readonly string[]): number {
  let total = 0;
  for (const line of lines) {
    if (line.startsWith("+") || line.startsWith("-")) {
      total += line.length;
    }
  }
  return total;
}

// browserFallbackMaxChangedLineBytes estimates the largest changed row in one generated diff.
// browserFallbackMaxChangedLineBytes 估算单个生成 diff 中最大的变更行体积。
function browserFallbackMaxChangedLineBytes(lines: readonly string[]): number {
  let max = 0;
  for (const line of lines) {
    if (line.startsWith("+") || line.startsWith("-")) {
      max = Math.max(max, line.length);
    }
  }
  return max;
}

// sumBrowserFallbackDiffMetric sums one numeric summary field across generated records.
// sumBrowserFallbackDiffMetric 汇总生成记录中的单个数值摘要字段。
function sumBrowserFallbackDiffMetric(
  records: readonly BrowserFallbackDiffFileRecord[],
  key: "additions" | "deletions" | "changedLines" | "changedBytes",
): number {
  let total = 0;
  for (const record of records) {
    total += record.summary[key];
  }
  return total;
}

// throwIfBrowserFallbackAborted mirrors abort behavior for browser preview service calls.
// throwIfBrowserFallbackAborted 为浏览器预览服务调用模拟中止行为。
function throwIfBrowserFallbackAborted(signal: AbortSignal | undefined): void {
  if (!signal?.aborted) {
    return;
  }
  const error = new Error("Browser preview diff request was aborted.");
  error.name = "AbortError";
  throw error;
}
