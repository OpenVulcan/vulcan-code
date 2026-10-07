import { DIFF_SEARCH_MAX_RESULTS, DIFF_SEARCH_PAGE_SIZE, DIFF_SEARCH_WORKER_SCAN_SLICE_LINE_LIMIT } from "./limits";
import type { DiffFile, DiffLine, DiffPanelSnapshot } from "./types";

// DiffSearchMatch stores one loaded diff-line search hit.
// DiffSearchMatch 保存一条已加载 diff 行搜索命中。
export interface DiffSearchMatch {
  // Stable key for React and active-result navigation.
  // 用于 React 与活跃结果导航的稳定键。
  key: string;
  // Stable file key that owns this match.
  // 拥有该命中的稳定文件键。
  fileKey: string;
  // File index in the current snapshot.
  // 当前快照中的文件索引。
  fileIndex: number;
  // Hunk index in the owning file.
  // 所属文件中的 hunk 索引。
  hunkIndex: number;
  // Line index in the owning hunk.
  // 所属 hunk 中的行索引。
  lineIndex: number;
  // Encoded line key shared with rendered row data attributes.
  // 与已渲染行 data 属性共享的编码行键。
  lineKey: string;
  // Display path of the owning file.
  // 所属文件的展示路径。
  filePath: string;
  // Matched diff line kind.
  // 命中的 diff 行类型。
  kind: DiffLine["kind"];
  // Old-file line number when available.
  // 可用时的旧文件行号。
  oldLine: number | null;
  // New-file line number when available.
  // 可用时的新文件行号。
  newLine: number | null;
  // Compact preview shown in search status or diagnostics.
  // 在搜索状态或诊断中展示的紧凑预览。
  preview: string;
}

// DiffSearchIndex stores paginated search state derived from loaded hunks.
// DiffSearchIndex 保存从已加载 hunk 推导出的分页搜索状态。
export interface DiffSearchIndex {
  // Normalized search query.
  // 规范化后的搜索查询。
  query: string;
  // Retained matches up to the configured cap.
  // 在配置上限内保留的命中结果。
  matches: DiffSearchMatch[];
  // Whether scanning stopped after reaching the match cap.
  // 是否因达到命中上限而停止扫描。
  truncated: boolean;
  // Number of loaded diff lines scanned.
  // 已扫描的已加载 diff 行数量。
  scannedLines: number;
}

// DiffSearchSliceOptions controls cooperative worker search scanning.
// DiffSearchSliceOptions 控制可协作让出的 Worker 搜索扫描。
export interface DiffSearchSliceOptions {
  // Returns false when the caller has superseded the current scan.
  // 当前扫描已被调用方取代时返回 false。
  shouldContinue?: () => boolean;
  // Yields control so the worker can receive newer messages between scan slices.
  // 让出执行权，使 Worker 可在扫描片段之间接收新消息。
  yieldToEventLoop?: () => Promise<void>;
  // Number of loaded diff lines scanned before each yield.
  // 每次让出前扫描的已加载 diff 行数量。
  lineSliceLimit?: number;
}

// DiffSearchFileKeyIndex stores ordered unique file keys and their positions.
// DiffSearchFileKeyIndex 保存有序去重文件 key 及其位置。
interface DiffSearchFileKeyIndex {
  // Ordered unique file keys from the search result.
  // 搜索结果中的有序去重文件 key。
  keys: string[];
  // File-key to unique-key index lookup.
  // 文件 key 到去重序号的查询表。
  indexByKey: Map<string, number>;
}

// DiffSearchMatchDerived stores match-wide lookup data built in one pass.
// DiffSearchMatchDerived 保存一次遍历构建的命中级查询数据。
interface DiffSearchMatchDerived {
  // Ordered unique file-key index derived from all matches.
  // 从全部命中派生的有序去重文件 key 索引。
  fileKeyIndex: DiffSearchFileKeyIndex;
  // Matched line keys grouped by owning file key.
  // 按所属文件 key 分组的命中行 key。
  lineKeysByFile: Map<string, ReadonlySet<string>>;
}

// DiffSearchVisiblePage stores all derived data needed to render the current search page.
// DiffSearchVisiblePage 保存渲染当前搜索分页所需的全部派生数据。
export interface DiffSearchVisiblePage {
  // Search matches in the active result page.
  // 当前结果分页内的搜索命中。
  matches: DiffSearchMatch[];
  // File keys in the active file projection page.
  // 当前文件投影分页内的文件 key。
  fileKeys: ReadonlySet<string>;
  // Matched line keys that should receive search classes.
  // 需要应用搜索样式的命中行 key。
  lineKeys: ReadonlySet<string>;
}

// EMPTY_DIFF_SEARCH_INDEX is the stable empty search state.
// EMPTY_DIFF_SEARCH_INDEX 是稳定的空搜索状态。
export const EMPTY_DIFF_SEARCH_INDEX: DiffSearchIndex = {
  query: "",
  matches: [],
  truncated: false,
  scannedLines: 0,
};

// EMPTY_DIFF_SEARCH_FILE_KEYS is the stable empty file-key set for search paging.
// EMPTY_DIFF_SEARCH_FILE_KEYS 是搜索分页使用的稳定空文件 key 集合。
const EMPTY_DIFF_SEARCH_FILE_KEYS: ReadonlySet<string> = new Set();

// EMPTY_DIFF_SEARCH_LINE_KEYS is the stable empty line-key set for search highlighting.
// EMPTY_DIFF_SEARCH_LINE_KEYS 是搜索高亮使用的稳定空行 key 集合。
const EMPTY_DIFF_SEARCH_LINE_KEYS: ReadonlySet<string> = new Set();

// EMPTY_DIFF_SEARCH_VISIBLE_PAGE is the stable visible-page descriptor for empty searches.
// EMPTY_DIFF_SEARCH_VISIBLE_PAGE 是空搜索复用的稳定可见分页描述符。
const EMPTY_DIFF_SEARCH_VISIBLE_PAGE: DiffSearchVisiblePage = {
  matches: [],
  fileKeys: EMPTY_DIFF_SEARCH_FILE_KEYS,
  lineKeys: EMPTY_DIFF_SEARCH_LINE_KEYS,
};

// normalizedDiffLineContentCache keeps lowercase line text for repeated worker searches.
// normalizedDiffLineContentCache 为重复 Worker 搜索保留小写后的行文本。
const normalizedDiffLineContentCache = new WeakMap<DiffLine, string>();

// diffSearchPageCache stores result pages by stable match arrays and page starts.
// diffSearchPageCache 按稳定命中数组与分页起点缓存搜索结果页。
const diffSearchPageCache = new WeakMap<readonly DiffSearchMatch[], Map<number, DiffSearchMatch[]>>();

// diffSearchMatchDerivedCache stores match-wide indexes for stable match arrays.
// diffSearchMatchDerivedCache 按稳定命中数组缓存命中级派生索引。
const diffSearchMatchDerivedCache = new WeakMap<readonly DiffSearchMatch[], DiffSearchMatchDerived>();

// diffSearchPageFileKeysCache stores file-page sets by stable match arrays and page starts.
// diffSearchPageFileKeysCache 按稳定命中数组与分页起点缓存文件分页集合。
const diffSearchPageFileKeysCache = new WeakMap<readonly DiffSearchMatch[], Map<number, ReadonlySet<string>>>();

// diffSearchVisiblePageCache stores current-page descriptors by numeric match and file page starts.
// diffSearchVisiblePageCache 按数值化命中分页起点与文件分页起点缓存当前分页描述符。
const diffSearchVisiblePageCache = new WeakMap<readonly DiffSearchMatch[], Map<number, Map<number, DiffSearchVisiblePage>>>();

// buildDiffSearchIndexSliced searches loaded hunk lines while yielding between bounded slices.
// buildDiffSearchIndexSliced 在有界片段之间让出执行权来搜索已加载 hunk 行。
export async function buildDiffSearchIndexSliced(
  snapshot: DiffPanelSnapshot,
  query: string,
  previousIndex?: DiffSearchIndex | null,
  options: DiffSearchSliceOptions = {},
): Promise<DiffSearchIndex | null> {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) {
    return EMPTY_DIFF_SEARCH_INDEX;
  }
  const lineSliceLimit = Math.max(1, options.lineSliceLimit ?? DIFF_SEARCH_WORKER_SCAN_SLICE_LINE_LIMIT);
  const refinedIndex = previousIndex
    ? await refinedDiffSearchIndexSliced(snapshot, normalizedQuery, previousIndex, lineSliceLimit, options)
    : null;
  if (refinedIndex) {
    return refinedIndex;
  }
  const matches: DiffSearchMatch[] = [];
  let scannedLines = 0;
  let linesSinceYield = 0;
  for (let fileIndex = 0; fileIndex < snapshot.files.length; fileIndex += 1) {
    const file = snapshot.files[fileIndex];
    for (let hunkIndex = 0; hunkIndex < file.hunks.length; hunkIndex += 1) {
      const hunk = file.hunks[hunkIndex];
      for (let lineIndex = 0; lineIndex < hunk.lines.length; lineIndex += 1) {
        if (options.shouldContinue && !options.shouldContinue()) {
          return null;
        }
        const line = hunk.lines[lineIndex];
        scannedLines += 1;
        linesSinceYield += 1;
        if (normalizedDiffLineContent(line).includes(normalizedQuery)) {
          matches.push(diffSearchMatchFromLine(file, fileIndex, hunkIndex, lineIndex, line));
          if (matches.length >= DIFF_SEARCH_MAX_RESULTS) {
            return { query: normalizedQuery, matches, truncated: true, scannedLines };
          }
        }
        if (linesSinceYield >= lineSliceLimit) {
          linesSinceYield = 0;
          if (options.yieldToEventLoop) {
            await options.yieldToEventLoop();
            if (options.shouldContinue && !options.shouldContinue()) {
              return null;
            }
          }
        }
      }
    }
  }
  return { query: normalizedQuery, matches, truncated: false, scannedLines };
}

// refinedDiffSearchIndexSliced filters a previous result while preserving worker preemption points.
// refinedDiffSearchIndexSliced 在保留 Worker 抢占边界的同时过滤上一份结果。
async function refinedDiffSearchIndexSliced(
  // Parsed diff snapshot that owns the previous matches.
  // 拥有上一份命中结果的已解析 diff snapshot。
  snapshot: DiffPanelSnapshot,
  // Already normalized query for the next search.
  // 下一次搜索已经规范化后的查询。
  normalizedQuery: string,
  // Previous complete search result from the same snapshot.
  // 同一 snapshot 的上一份完整搜索结果。
  previousIndex: DiffSearchIndex,
  // Number of retained matches checked before each cooperative yield.
  // 每次协作让出前检查的保留命中数量。
  matchSliceLimit: number,
  // Cooperative worker search options.
  // 协作式 Worker 搜索选项。
  options: DiffSearchSliceOptions,
): Promise<DiffSearchIndex | null> {
  if (previousIndex.query === normalizedQuery) {
    return previousIndex;
  }
  if (!previousIndex.query || previousIndex.truncated || !normalizedQuery.startsWith(previousIndex.query)) {
    return null;
  }
  const matches: DiffSearchMatch[] = [];
  let matchesSinceYield = 0;
  for (const match of previousIndex.matches) {
    if (options.shouldContinue && !options.shouldContinue()) {
      return null;
    }
    const line = diffLineForSearchMatch(snapshot, match);
    if (!line) {
      return null;
    }
    matchesSinceYield += 1;
    if (normalizedDiffLineContent(line).includes(normalizedQuery)) {
      matches.push(match);
    }
    if (matchesSinceYield >= matchSliceLimit) {
      matchesSinceYield = 0;
      if (options.yieldToEventLoop) {
        await options.yieldToEventLoop();
        if (options.shouldContinue && !options.shouldContinue()) {
          return null;
        }
      }
    }
  }
  return {
    query: normalizedQuery,
    matches,
    truncated: false,
    scannedLines: previousIndex.scannedLines,
  };
}

// diffLineForSearchMatch resolves a retained search match back to its parsed line.
// diffLineForSearchMatch 将保留的搜索命中解析回对应 diff 行。
function diffLineForSearchMatch(snapshot: DiffPanelSnapshot, match: DiffSearchMatch): DiffLine | null {
  return snapshot.files[match.fileIndex]?.hunks[match.hunkIndex]?.lines[match.lineIndex] ?? null;
}

// normalizedDiffLineContent returns cached lowercase text for one parsed diff line.
// normalizedDiffLineContent 返回单条已解析 diff 行的小写文本缓存。
function normalizedDiffLineContent(line: DiffLine): string {
  const cached = normalizedDiffLineContentCache.get(line);
  if (cached != null) {
    return cached;
  }
  const normalized = line.content.toLowerCase();
  normalizedDiffLineContentCache.set(line, normalized);
  return normalized;
}

// diffSearchVisiblePage returns one descriptor for the active search match and file pages.
// diffSearchVisiblePage 返回活跃搜索命中分页与文件分页的统一描述符。
export function diffSearchVisiblePage(matches: DiffSearchMatch[], activeIndex: number): DiffSearchVisiblePage {
  if (!matches.length) {
    return EMPTY_DIFF_SEARCH_VISIBLE_PAGE;
  }
  const safeIndex = clampDiffSearchIndex(activeIndex, matches.length);
  const matchPageStart = Math.floor(safeIndex / DIFF_SEARCH_PAGE_SIZE) * DIFF_SEARCH_PAGE_SIZE;
  const derived = diffSearchMatchDerived(matches);
  const activeMatch = matches[safeIndex];
  const activeFileIndex = derived.fileKeyIndex.indexByKey.get(activeMatch?.fileKey ?? "") ?? 0;
  const filePageStart = Math.floor(activeFileIndex / DIFF_SEARCH_PAGE_SIZE) * DIFF_SEARCH_PAGE_SIZE;
  let visiblePagesByStart = diffSearchVisiblePageCache.get(matches);
  if (!visiblePagesByStart) {
    visiblePagesByStart = new Map();
    diffSearchVisiblePageCache.set(matches, visiblePagesByStart);
  }
  let visiblePagesByFileStart = visiblePagesByStart.get(matchPageStart);
  if (!visiblePagesByFileStart) {
    visiblePagesByFileStart = new Map();
    visiblePagesByStart.set(matchPageStart, visiblePagesByFileStart);
  }
  const cached = visiblePagesByFileStart.get(filePageStart);
  if (cached) {
    return cached;
  }
  const pageMatches = diffSearchPageByStart(matches, matchPageStart);
  const fileKeys = cachedDiffSearchPageFileKeysByStart(matches, derived.fileKeyIndex, filePageStart);
  const lineKeys = diffSearchLineKeysForFileSet(matches, fileKeys, derived.lineKeysByFile);
  const visiblePage = { matches: pageMatches, fileKeys, lineKeys };
  visiblePagesByFileStart.set(filePageStart, visiblePage);
  return visiblePage;
}

// diffSearchPageByStart returns a cached match page for one precomputed page start.
// diffSearchPageByStart 返回指定预计算分页起点对应的缓存命中页。
function diffSearchPageByStart(matches: DiffSearchMatch[], pageStart: number): DiffSearchMatch[] {
  let pageByStart = diffSearchPageCache.get(matches);
  if (!pageByStart) {
    pageByStart = new Map();
    diffSearchPageCache.set(matches, pageByStart);
  }
  const cached = pageByStart.get(pageStart);
  if (cached) {
    return cached;
  }
  const pageEnd = Math.min(matches.length, pageStart + DIFF_SEARCH_PAGE_SIZE);
  const page: DiffSearchMatch[] = new Array(pageEnd - pageStart);
  for (let index = pageStart; index < pageEnd; index += 1) {
    page[index - pageStart] = matches[index];
  }
  pageByStart.set(pageStart, page);
  return page;
}

// cachedDiffSearchPageFileKeysByStart returns a shared file-key page for all search page consumers.
// cachedDiffSearchPageFileKeysByStart 为所有搜索分页消费者返回共享的文件 key 分页。
function cachedDiffSearchPageFileKeysByStart(
  // Stable match array that owns the cache.
  // 拥有缓存的稳定命中数组。
  matches: DiffSearchMatch[],
  // Ordered unique file-key index.
  // 有序去重文件 key 索引。
  uniqueFileKeyIndex: DiffSearchFileKeyIndex,
  // File page start index.
  // 文件分页起始索引。
  pageStart: number,
): ReadonlySet<string> {
  let fileKeysByStart = diffSearchPageFileKeysCache.get(matches);
  if (!fileKeysByStart) {
    fileKeysByStart = new Map();
    diffSearchPageFileKeysCache.set(matches, fileKeysByStart);
  }
  const cached = fileKeysByStart.get(pageStart);
  if (cached) {
    return cached;
  }
  const fileKeys = diffSearchPageFileKeysByStart(uniqueFileKeyIndex, pageStart);
  fileKeysByStart.set(pageStart, fileKeys);
  return fileKeys;
}

// diffSearchPageFileKeysByStart returns cached file keys for one precomputed file-page start.
// diffSearchPageFileKeysByStart 返回指定预计算文件分页起点对应的缓存文件 key。
function diffSearchPageFileKeysByStart(
  uniqueFileKeyIndex: DiffSearchFileKeyIndex,
  pageStart: number,
): ReadonlySet<string> {
  const fileKeys = new Set<string>();
  const pageEnd = Math.min(uniqueFileKeyIndex.keys.length, pageStart + DIFF_SEARCH_PAGE_SIZE);
  for (let index = pageStart; index < pageEnd; index += 1) {
    fileKeys.add(uniqueFileKeyIndex.keys[index]);
  }
  return fileKeys;
}

// diffSearchLineKeysForFileSet returns matched line keys for one file-key set and grouped line index.
// diffSearchLineKeysForFileSet 基于文件 key 集合与行分组索引返回命中行 key。
function diffSearchLineKeysForFileSet(
  matches: DiffSearchMatch[],
  fileKeys: ReadonlySet<string>,
  lineKeysByFile: Map<string, ReadonlySet<string>>,
): ReadonlySet<string> {
  if (fileKeys.size === 1) {
    const onlyFileKey = firstDiffSearchFileKey(fileKeys);
    return onlyFileKey ? lineKeysByFile.get(onlyFileKey) ?? EMPTY_DIFF_SEARCH_LINE_KEYS : EMPTY_DIFF_SEARCH_LINE_KEYS;
  }
  const lineKeys = new Set<string>();
  for (const fileKey of fileKeys) {
    const fileLineKeys = lineKeysByFile.get(fileKey);
    if (!fileLineKeys) {
      continue;
    }
    for (const lineKey of fileLineKeys) {
      lineKeys.add(lineKey);
    }
  }
  return lineKeys;
}

// firstDiffSearchFileKey returns the first file key from a readonly set.
// firstDiffSearchFileKey 从只读集合中返回第一个文件 key。
function firstDiffSearchFileKey(fileKeys: ReadonlySet<string>): string | null {
  for (const fileKey of fileKeys) {
    return fileKey;
  }
  return null;
}

// clampDiffSearchIndex keeps active search navigation inside available matches.
// clampDiffSearchIndex 将活跃搜索索引限制在可用命中范围内。
export function clampDiffSearchIndex(index: number, matchCount: number): number {
  if (matchCount <= 0) {
    return 0;
  }
  return Math.max(0, Math.min(index, matchCount - 1));
}

// diffSearchLineKey builds a DOM-safe key for one diff line.
// diffSearchLineKey 为单条 diff 行构建 DOM 安全键。
export function diffSearchLineKey(fileKey: string, hunkIndex: number, lineIndex: number): string {
  return encodeURIComponent(`${fileKey}\u0000${hunkIndex}\u0000${lineIndex}`);
}

// diffSearchMatchDerived returns combined match-wide indexes from one cached pass.
// diffSearchMatchDerived 通过一次缓存遍历返回合并后的命中级索引。
function diffSearchMatchDerived(matches: readonly DiffSearchMatch[]): DiffSearchMatchDerived {
  const cached = diffSearchMatchDerivedCache.get(matches);
  if (cached) {
    return cached;
  }
  const keys: string[] = [];
  const indexByKey = new Map<string, number>();
  const lineKeysByFile = new Map<string, Set<string>>();
  for (const match of matches) {
    if (!indexByKey.has(match.fileKey)) {
      indexByKey.set(match.fileKey, keys.length);
      keys.push(match.fileKey);
    }
    let lineKeys = lineKeysByFile.get(match.fileKey);
    if (!lineKeys) {
      lineKeys = new Set();
      lineKeysByFile.set(match.fileKey, lineKeys);
    }
    lineKeys.add(match.lineKey);
  }
  const derived = {
    fileKeyIndex: { keys, indexByKey },
    lineKeysByFile,
  };
  diffSearchMatchDerivedCache.set(matches, derived);
  return derived;
}

// diffSearchMatchFromLine converts one matching line into a navigation record.
// diffSearchMatchFromLine 将一条命中行转换为导航记录。
function diffSearchMatchFromLine(
  file: DiffFile,
  fileIndex: number,
  hunkIndex: number,
  lineIndex: number,
  line: DiffLine,
): DiffSearchMatch {
  const lineKey = diffSearchLineKey(file.key, hunkIndex, lineIndex);
  return {
    key: lineKey,
    fileKey: file.key,
    fileIndex,
    hunkIndex,
    lineIndex,
    lineKey,
    filePath: displayPath(file),
    kind: line.kind,
    oldLine: line.oldLine,
    newLine: line.newLine,
    preview: compactPreview(line.content),
  };
}

// displayPath returns the preferred file path for one search match.
// displayPath 返回单条搜索命中的首选文件路径。
function displayPath(file: DiffFile): string {
  return file.newPath && file.newPath !== "/dev/null" ? file.newPath : file.oldPath;
}

// compactPreview trims one diff line to a small single-line preview.
// compactPreview 将单条 diff 行裁剪为较短的单行预览。
function compactPreview(content: string): string {
  const normalized = content.replace(/\s+/g, " ").trim();
  if (normalized.length <= 140) {
    return normalized;
  }
  return `${normalized.slice(0, 137)}...`;
}
