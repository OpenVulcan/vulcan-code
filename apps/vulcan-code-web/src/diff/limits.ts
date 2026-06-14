import type { DiffFile, DiffStats } from "./types";

// DIFF_TOTAL_FILES_LIMIT mirrors the target renderer's large-diff file-count gate.
// DIFF_TOTAL_FILES_LIMIT 对齐目标渲染器的大 diff 文件数量阈值。
export const DIFF_TOTAL_FILES_LIMIT = 128;

// DIFF_TOTAL_CHANGED_LINES_LIMIT mirrors the target renderer's aggregate changed-line gate.
// DIFF_TOTAL_CHANGED_LINES_LIMIT 对齐目标渲染器的总变更行数阈值。
export const DIFF_TOTAL_CHANGED_LINES_LIMIT = 9000;

// DIFF_TOTAL_CHANGED_BYTES_LIMIT mirrors the target renderer's aggregate changed-byte gate.
// DIFF_TOTAL_CHANGED_BYTES_LIMIT 对齐目标渲染器的总变更字节数阈值。
export const DIFF_TOTAL_CHANGED_BYTES_LIMIT = 12 * 1024 * 1024;

// DIFF_FILE_CHANGED_LINES_LIMIT mirrors the target renderer's per-file changed-line gate.
// DIFF_FILE_CHANGED_LINES_LIMIT 对齐目标渲染器的单文件变更行数阈值。
export const DIFF_FILE_CHANGED_LINES_LIMIT = 15000;

// DIFF_FILE_CHANGED_BYTES_LIMIT mirrors the target renderer's per-file changed-byte gate.
// DIFF_FILE_CHANGED_BYTES_LIMIT 对齐目标渲染器的单文件变更字节数阈值。
export const DIFF_FILE_CHANGED_BYTES_LIMIT = 3 * 1024 * 1024;

// DIFF_FILE_MAX_LINE_BYTES_LIMIT mirrors the target renderer's single-line byte gate.
// DIFF_FILE_MAX_LINE_BYTES_LIMIT 对齐目标渲染器的单行字节数阈值。
export const DIFF_FILE_MAX_LINE_BYTES_LIMIT = 1024 * 1024;

// DIFF_LIGHTWEIGHT_DETAIL_CHANGED_LINES_LIMIT skips expensive inline work for mid-sized files.
// DIFF_LIGHTWEIGHT_DETAIL_CHANGED_LINES_LIMIT 对中等偏大的文件省略昂贵的行内处理。
export const DIFF_LIGHTWEIGHT_DETAIL_CHANGED_LINES_LIMIT = 2000;

// DIFF_SHIKI_TOKEN_ROW_LIMIT caps Shiki tokenization inside the new worker renderer pipeline.
// DIFF_SHIKI_TOKEN_ROW_LIMIT 限制新 Worker 渲染流水线内的 Shiki token 化行数。
export const DIFF_SHIKI_TOKEN_ROW_LIMIT = 2000;

// DIFF_SEARCH_PAGE_SIZE mirrors the target search result pagination size.
// DIFF_SEARCH_PAGE_SIZE 对齐目标搜索结果分页大小。
export const DIFF_SEARCH_PAGE_SIZE = 20;

// DIFF_SEARCH_MAX_RESULTS bounds in-memory search result retention for large diffs.
// DIFF_SEARCH_MAX_RESULTS 限制大 diff 下内存中保留的搜索结果数量。
export const DIFF_SEARCH_MAX_RESULTS = 2000;

// DIFF_SEARCH_DETAIL_PREFETCH_LIMIT caps summary-only file details loaded per search cycle.
// DIFF_SEARCH_DETAIL_PREFETCH_LIMIT 限制每轮搜索最多预取的摘要文件详情数量。
export const DIFF_SEARCH_DETAIL_PREFETCH_LIMIT = 4;

// DIFF_SEARCH_DETAIL_PREFETCH_DISPATCH_DELAY_MS spaces backend detail prefetches across frames.
// DIFF_SEARCH_DETAIL_PREFETCH_DISPATCH_DELAY_MS 将后端详情预取分散到多个帧中派发。
export const DIFF_SEARCH_DETAIL_PREFETCH_DISPATCH_DELAY_MS = 48;

// DIFF_SEARCH_DETAIL_PREFETCH_CONTEXT_LINES requests a focused expanded hunk for summary hits.
// DIFF_SEARCH_DETAIL_PREFETCH_CONTEXT_LINES 为摘要命中请求聚焦的扩展 hunk。
export const DIFF_SEARCH_DETAIL_PREFETCH_CONTEXT_LINES = 20;

// DIFF_SEARCH_DETAIL_PREFETCH_TARGET_MATCHES keeps searching summary-only files until one visible page can fill.
// DIFF_SEARCH_DETAIL_PREFETCH_TARGET_MATCHES 让摘要文件搜索预取至少尝试补齐一个可见结果页。
export const DIFF_SEARCH_DETAIL_PREFETCH_TARGET_MATCHES = DIFF_SEARCH_PAGE_SIZE;

// DIFF_SEARCH_SUMMARY_FILE_MATCH_LIMIT caps backend file-hit metadata used for search hydration.
// DIFF_SEARCH_SUMMARY_FILE_MATCH_LIMIT 限制搜索水合使用的后端文件命中元数据数量。
export const DIFF_SEARCH_SUMMARY_FILE_MATCH_LIMIT = DIFF_SEARCH_DETAIL_PREFETCH_TARGET_MATCHES * 10;

// DIFF_SEARCH_SUMMARY_REQUEST_DEBOUNCE_MS delays backend full-diff search while the user is still typing.
// DIFF_SEARCH_SUMMARY_REQUEST_DEBOUNCE_MS 在用户仍在输入时延迟后端完整 diff 搜索。
export const DIFF_SEARCH_SUMMARY_REQUEST_DEBOUNCE_MS = 160;

// DIFF_SEARCH_WORKER_REQUEST_DEBOUNCE_MS coalesces rapid local search input before worker scanning starts.
// DIFF_SEARCH_WORKER_REQUEST_DEBOUNCE_MS 在 Worker 扫描开始前合并快速本地搜索输入。
export const DIFF_SEARCH_WORKER_REQUEST_DEBOUNCE_MS = 80;

// DIFF_SEARCH_WORKER_SCAN_SLICE_LINE_LIMIT yields worker search after this many scanned lines.
// DIFF_SEARCH_WORKER_SCAN_SLICE_LINE_LIMIT 每扫描这么多行后让出 Worker 搜索执行权。
export const DIFF_SEARCH_WORKER_SCAN_SLICE_LINE_LIMIT = 1200;

// DIFF_SEARCH_WORKER_INDEX_CACHE_LIMIT caps exact-query indexes retained inside the search worker.
// DIFF_SEARCH_WORKER_INDEX_CACHE_LIMIT 限制搜索 Worker 内保留的精确查询索引数量。
export const DIFF_SEARCH_WORKER_INDEX_CACHE_LIMIT = 16;

// DIFF_PARSE_WORKER_PROMISE_CACHE_LIMIT caps repeated raw-diff parse promises retained on the main thread.
// DIFF_PARSE_WORKER_PROMISE_CACHE_LIMIT 限制主线程保留的重复原始 diff 解析 Promise 数量。
export const DIFF_PARSE_WORKER_PROMISE_CACHE_LIMIT = 8;

// DIFF_FILE_VIRTUALIZATION_THRESHOLD enables row virtualization for large file bodies.
// DIFF_FILE_VIRTUALIZATION_THRESHOLD 为较大的文件正文启用行虚拟化。
export const DIFF_FILE_VIRTUALIZATION_THRESHOLD = 400;

// DIFF_LIST_VIRTUALIZATION_FILE_LIMIT enables file-card virtualization for many-file diffs.
// DIFF_LIST_VIRTUALIZATION_FILE_LIMIT 为多文件 diff 启用文件卡片虚拟化。
export const DIFF_LIST_VIRTUALIZATION_FILE_LIMIT = 24;

// DIFF_LIST_VIRTUALIZATION_ROW_LIMIT enables file-card virtualization for many rendered rows.
// DIFF_LIST_VIRTUALIZATION_ROW_LIMIT 为多行 diff 启用文件卡片虚拟化。
export const DIFF_LIST_VIRTUALIZATION_ROW_LIMIT = 1800;

// DIFF_LIST_CARD_HEADER_HEIGHT is the estimated header height used by file-list virtualization.
// DIFF_LIST_CARD_HEADER_HEIGHT 是文件列表虚拟化使用的文件头估算高度。
export const DIFF_LIST_CARD_HEADER_HEIGHT = 40;

// DIFF_LIST_CARD_VERTICAL_GAP is the estimated vertical gap between file cards.
// DIFF_LIST_CARD_VERTICAL_GAP 是文件卡片之间的估算垂直间距。
export const DIFF_LIST_CARD_VERTICAL_GAP = 0;

// DIFF_LINE_HEIGHT is the fixed visual height for one diff code row.
// DIFF_LINE_HEIGHT 是单条 diff 代码行的固定视觉高度。
export const DIFF_LINE_HEIGHT = 20;

// DIFF_HUNK_HEADER_HEIGHT is the fixed visual height for one hunk header row.
// DIFF_HUNK_HEADER_HEIGHT 是单条 hunk 头部行的固定视觉高度。
export const DIFF_HUNK_HEADER_HEIGHT = 32;

// DIFF_RENDER_POLICY_NOTE_HEIGHT is the fixed height used by large-file policy notes.
// DIFF_RENDER_POLICY_NOTE_HEIGHT 是大文件渲染策略提示使用的固定高度。
export const DIFF_RENDER_POLICY_NOTE_HEIGHT = 31;

// DIFF_SUMMARY_PLACEHOLDER_MIN_HEIGHT keeps summary-only placeholders readable and height-stable.
// DIFF_SUMMARY_PLACEHOLDER_MIN_HEIGHT 保持 summary-only 占位可读且高度稳定。
export const DIFF_SUMMARY_PLACEHOLDER_MIN_HEIGHT = 88;

// DIFF_VIRTUAL_OVERSCAN_PX keeps nearby rows mounted to avoid blank edges during fast scroll.
// DIFF_VIRTUAL_OVERSCAN_PX 保留视口附近行，避免快速滚动时出现空白边缘。
export const DIFF_VIRTUAL_OVERSCAN_PX = 900;

// DIFF_SCROLL_STATE_GRANULARITY_PX coalesces pixel-level scroll updates before React virtualization work.
// DIFF_SCROLL_STATE_GRANULARITY_PX 在 React 虚拟化计算前合并像素级滚动更新。
export const DIFF_SCROLL_STATE_GRANULARITY_PX = 8;

// LARGE_DIFF_REASON_LABELS maps internal reason ids to compact Chinese descriptions.
// LARGE_DIFF_REASON_LABELS 将内部原因标识映射为紧凑中文描述。
export const LARGE_DIFF_REASON_LABELS: Record<string, string> = {
  total_files: `文件数超过 ${DIFF_TOTAL_FILES_LIMIT}`,
  total_changed_lines: `总变更行超过 ${DIFF_TOTAL_CHANGED_LINES_LIMIT}`,
  total_changed_bytes: `总变更体积超过 ${formatBytes(DIFF_TOTAL_CHANGED_BYTES_LIMIT)}`,
  file_changed_lines: `单文件变更行超过 ${DIFF_FILE_CHANGED_LINES_LIMIT}`,
  file_changed_bytes: `单文件变更体积超过 ${formatBytes(DIFF_FILE_CHANGED_BYTES_LIMIT)}`,
  file_max_line_bytes: `单行体积超过 ${formatBytes(DIFF_FILE_MAX_LINE_BYTES_LIMIT)}`,
};

// diffFileLargeReasons returns target-compatible large-file reasons for one parsed file.
// diffFileLargeReasons 返回单个已解析文件的目标兼容大文件原因。
export function diffFileLargeReasons(file: Pick<DiffFile, "changedLines" | "changedBytes" | "maxChangedLineBytes">): string[] {
  const reasons: string[] = [];
  if (file.changedLines > DIFF_FILE_CHANGED_LINES_LIMIT) {
    reasons.push("file_changed_lines");
  }
  if (file.changedBytes > DIFF_FILE_CHANGED_BYTES_LIMIT) {
    reasons.push("file_changed_bytes");
  }
  if (file.maxChangedLineBytes > DIFF_FILE_MAX_LINE_BYTES_LIMIT) {
    reasons.push("file_max_line_bytes");
  }
  return reasons;
}

// diffStatsLargeReasons returns target-compatible large-diff reasons for aggregate stats.
// diffStatsLargeReasons 返回聚合统计的目标兼容大 diff 原因。
export function diffStatsLargeReasons(stats: Pick<DiffStats, "files" | "totalChangedLines" | "totalChangedBytes">): string[] {
  const reasons: string[] = [];
  if (stats.files > DIFF_TOTAL_FILES_LIMIT) {
    reasons.push("total_files");
  }
  if (stats.totalChangedLines > DIFF_TOTAL_CHANGED_LINES_LIMIT) {
    reasons.push("total_changed_lines");
  }
  if (stats.totalChangedBytes > DIFF_TOTAL_CHANGED_BYTES_LIMIT) {
    reasons.push("total_changed_bytes");
  }
  return reasons;
}

// formatLargeReasons turns reason ids into display labels with stable fallbacks.
// formatLargeReasons 将原因标识转换为显示标签，并提供稳定兜底。
export function formatLargeReasons(reasons: readonly string[]): string {
  return reasons.map((reason) => LARGE_DIFF_REASON_LABELS[reason] ?? reason).join("、");
}

// shouldUseLightweightDiffRendering returns whether expensive detail rendering should be skipped.
// shouldUseLightweightDiffRendering 返回是否应该省略昂贵的细节渲染。
export function shouldUseLightweightDiffRendering(file: Pick<DiffFile, "changedLines" | "isLarge" | "isBinary" | "isGitlink">): boolean {
  return file.isBinary
    || file.isGitlink
    || file.isLarge
    || file.changedLines > DIFF_LIGHTWEIGHT_DETAIL_CHANGED_LINES_LIMIT;
}

// formatBytes formats byte thresholds for compact UI notes.
// formatBytes 将字节阈值格式化为紧凑界面提示。
function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    return `${Math.round(bytes / 1024 / 1024)} MiB`;
  }
  if (bytes >= 1024) {
    return `${Math.round(bytes / 1024)} KiB`;
  }
  return `${bytes} B`;
}
