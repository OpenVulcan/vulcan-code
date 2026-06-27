import type { DiffFile, DiffHunk, DiffInlineChangeRange, DiffLine, DiffPanelSnapshot, DiffPanelSource } from "./types";
import { diffFileLargeReasons, diffStatsLargeReasons } from "./limits";

// textEncoder estimates UTF-8 byte sizes for changed diff lines.
// textEncoder 用于估算 diff 变更行的 UTF-8 字节体积。
const textEncoder = new TextEncoder();

// gitQuotedPathDecoder decodes Git C-style octal path byte escapes.
// gitQuotedPathDecoder 用于解码 Git C 风格八进制路径字节转义。
const gitQuotedPathDecoder = new TextDecoder();

// INLINE_DIFF_MAX_LINE_LENGTH bounds parser-time character range detection.
// INLINE_DIFF_MAX_LINE_LENGTH 限制解析阶段行内变更范围检测的单行长度。
const INLINE_DIFF_MAX_LINE_LENGTH = 4000;

// INLINE_DIFF_MAX_TOKEN_COUNT bounds parser-time token LCS work.
// INLINE_DIFF_MAX_TOKEN_COUNT 限制解析阶段 token LCS 的工作量。
const INLINE_DIFF_MAX_TOKEN_COUNT = 240;

// INLINE_DIFF_MAX_RUN_PAIR_CELLS bounds parser-time changed-run line pairing work.
// INLINE_DIFF_MAX_RUN_PAIR_CELLS 限制解析阶段变更块行配对的计算量。
const INLINE_DIFF_MAX_RUN_PAIR_CELLS = 4096;

// INLINE_DIFF_MIN_LINE_SIMILARITY is the minimum weighted score for pairing changed lines.
// INLINE_DIFF_MIN_LINE_SIMILARITY 是配对变更行所需的最小加权相似度。
const INLINE_DIFF_MIN_LINE_SIMILARITY = 260;

// InlineDiffToken stores one token and its UTF-16 code-unit range.
// InlineDiffToken 保存一个 token 及其 UTF-16 code unit 范围。
interface InlineDiffToken {
  // Token text used for LCS equality.
  // 用于 LCS 相等判断的 token 文本。
  text: string;
  // Inclusive token start offset.
  // token 的包含式起始偏移。
  start: number;
  // Exclusive token end offset.
  // token 的排他式结束偏移。
  end: number;
}

// InlineDiffTokenPair stores one unchanged token pair from LCS.
// InlineDiffTokenPair 保存 LCS 得到的一对未变化 token。
interface InlineDiffTokenPair {
  // Index in the removed token list.
  // 删除 token 列表中的索引。
  removedIndex: number;
  // Index in the added token list.
  // 新增 token 列表中的索引。
  addedIndex: number;
}

// InlineDiffLinePair stores one matched removed/added row pair inside a changed run.
// InlineDiffLinePair 保存变更块内一对匹配的删除/新增行。
interface InlineDiffLinePair {
  // Index in the hunk line array for the removed row.
  // 删除行在 hunk 行数组中的下标。
  removedIndex: number;
  // Index in the hunk line array for the added row.
  // 新增行在 hunk 行数组中的下标。
  addedIndex: number;
}

// InlineDiffLineSimilarityMatrix stores a flat bounded score matrix for changed-run pairing.
// InlineDiffLineSimilarityMatrix 保存变更块行配对使用的扁平有界分数矩阵。
interface InlineDiffLineSimilarityMatrix {
  // Flat row-major similarity scores.
  // 按行优先存储的扁平相似度分数。
  scores: Uint16Array;
  // Number of removed rows represented by the matrix.
  // 矩阵表示的删除行数量。
  removedCount: number;
  // Number of added rows represented by the matrix.
  // 矩阵表示的新增行数量。
  addedCount: number;
}

// HunkCursor tracks old and new line numbers while parsing hunk rows.
// HunkCursor 在解析 hunk 行时跟踪旧文件与新文件行号。
interface HunkCursor {
  // Current old-file line number.
  // 当前旧文件行号。
  oldLine: number;
  // Current new-file line number.
  // 当前新文件行号。
  newLine: number;
}

// parseUnifiedDiff converts one raw unified diff string into renderer-ready records.
// parseUnifiedDiff 将原始 unified diff 字符串转换为可渲染记录。
export function parseUnifiedDiff(rawDiff: string, source: DiffPanelSource = "message", repositoryRoot = ""): DiffPanelSnapshot {
  const files: DiffFile[] = [];
  let currentFile: DiffFile | null = null;
  let currentHunk: DiffHunk | null = null;
  let currentCursor: HunkCursor | null = null;
  let currentFileRawStart = 0;
  let currentFileRawEnd = 0;

  // flushHunk commits the current hunk into the active file.
  // flushHunk 将当前 hunk 提交到活跃文件中。
  const flushHunk = () => {
    if (currentFile && currentHunk) {
      applyInlineChangeRanges(currentHunk);
      currentFile.rowCount = (currentFile.rowCount ?? 0) + 1 + currentHunk.lines.length;
      currentFile.hunks.push(currentHunk);
    }
    currentHunk = null;
    currentCursor = null;
  };

  // flushFile commits the current file into the output list.
  // flushFile 将当前文件提交到输出列表中。
  const flushFile = () => {
    flushHunk();
    if (currentFile) {
      currentFile.status = finalFileStatus(currentFile);
      currentFile.rawDiff = rawDiff.slice(currentFileRawStart, currentFileRawEnd);
      files.push(currentFile);
    }
    currentFile = null;
    currentFileRawStart = 0;
    currentFileRawEnd = 0;
  };

  // consumeRawDiffLine handles one normalized line without materializing the whole diff line array.
  // consumeRawDiffLine 处理单条归一化行，避免为整份 diff 物化行数组。
  const consumeRawDiffLine = (rawLine: string, rawLineStart: number, nextLineStart: number) => {
    if (rawLine.startsWith("diff --git ")) {
      flushFile();
      const [oldPath, newPath] = parseDiffGitHeader(rawLine);
      currentFileRawStart = rawLineStart;
      currentFileRawEnd = nextLineStart;
      currentFile = {
        key: `${oldPath}->${newPath}:${files.length}`,
        oldPath,
        newPath,
        status: "modified",
        additions: 0,
        deletions: 0,
        isBinary: false,
        isGitlink: false,
        fileMode: null,
        changedLines: 0,
        changedBytes: 0,
        maxChangedLineBytes: 0,
        isLarge: false,
        largeReasons: [],
        rowCount: 0,
        hunks: [],
      };
      return;
    }

    if (!currentFile) {
      return;
    }

    const activeFile = currentFile;
    currentFileRawEnd = nextLineStart;

    if (rawLine.startsWith("new file mode")) {
      activeFile.status = "added";
      applyFileMode(activeFile, rawLine.slice("new file mode".length).trim());
      return;
    }
    if (rawLine.startsWith("deleted file mode")) {
      activeFile.status = "deleted";
      applyFileMode(activeFile, rawLine.slice("deleted file mode".length).trim());
      return;
    }
    if (rawLine.startsWith("old mode ") || rawLine.startsWith("new mode ")) {
      applyFileMode(activeFile, lastWhitespaceSeparatedToken(rawLine));
      return;
    }
    if (rawLine.startsWith("index ")) {
      applyIndexLineMode(activeFile, rawLine);
      return;
    }
    if (rawLine.startsWith("rename from ")) {
      activeFile.oldPath = trimDiffPath(rawLine.slice("rename from ".length));
      activeFile.status = "renamed";
      return;
    }
    if (rawLine.startsWith("rename to ")) {
      activeFile.newPath = trimDiffPath(rawLine.slice("rename to ".length));
      activeFile.status = "renamed";
      return;
    }
    if (rawLine.startsWith("Binary files ") || rawLine.startsWith("GIT binary patch")) {
      activeFile.isBinary = true;
      activeFile.status = "binary";
      return;
    }
    if (rawLine.startsWith("@@ ")) {
      flushHunk();
      currentHunk = parseHunkHeader(rawLine);
      currentCursor = { oldLine: currentHunk.oldStart, newLine: currentHunk.newStart };
      return;
    }
    if (currentHunk && currentCursor) {
      const line = parseDiffLine(rawLine, currentCursor);
      applyLineCounters(activeFile, currentHunk, line);
      advanceCursor(currentCursor, line);
    }
  };

  forEachUnifiedDiffLine(rawDiff, consumeRawDiffLine);
  flushFile();
  const stats = diffStatsFromFiles(files);
  return {
    source,
    repositoryRoot,
    files,
    stats,
    completeness: "complete",
    isTruncated: false,
    rawDiff,
    notice: files.length ? undefined : "当前没有可展示的 Git Diff。",
  };
}

// forEachUnifiedDiffLine scans a raw diff line by line without building a full split array.
// forEachUnifiedDiffLine 逐行扫描原始 diff，避免构建完整 split 数组。
function forEachUnifiedDiffLine(rawDiff: string, consumeLine: (line: string, rawLineStart: number, nextLineStart: number) => void): void {
  let lineStart = 0;
  for (let index = 0; index < rawDiff.length; index += 1) {
    if (rawDiff.charCodeAt(index) !== 10) {
      continue;
    }
    const lineEnd = index > lineStart && rawDiff.charCodeAt(index - 1) === 13 ? index - 1 : index;
    consumeLine(rawDiff.slice(lineStart, lineEnd), lineStart, index + 1);
    lineStart = index + 1;
  }
  if (lineStart < rawDiff.length || rawDiff.length === 0) {
    consumeLine(rawDiff.slice(lineStart), lineStart, rawDiff.length);
  }
}

// diffStatsFromFiles aggregates parsed file metrics without per-file accumulator objects.
// diffStatsFromFiles 聚合已解析文件指标，避免为每个文件创建累加对象。
function diffStatsFromFiles(files: readonly DiffFile[]): DiffPanelSnapshot["stats"] {
  let additions = 0;
  let deletions = 0;
  let totalChangedLines = 0;
  let totalChangedBytes = 0;
  for (const file of files) {
    additions += file.additions;
    deletions += file.deletions;
    totalChangedLines += file.changedLines;
    totalChangedBytes += file.changedBytes;
  }
  const baseStats = {
    files: files.length,
    additions,
    deletions,
    totalChangedLines,
    totalChangedBytes,
  };
  const largeReasons = diffStatsLargeReasons(baseStats);
  return {
    ...baseStats,
    isLarge: largeReasons.length > 0,
    largeReasons,
  };
}

// hasRenderableDiff returns whether a text payload looks like a git unified diff.
// hasRenderableDiff 返回文本载荷是否看起来像 git unified diff。
export function hasRenderableDiff(rawDiff: string): boolean {
  return /^\s*diff --git /m.test(rawDiff) || /^\s*@@ -\d+/m.test(rawDiff);
}

// parseDiffGitHeader parses a standard `diff --git` header into old and new paths.
// parseDiffGitHeader 将标准 `diff --git` 头解析为旧路径和新路径。
function parseDiffGitHeader(header: string): [string, string] {
  const body = header.slice("diff --git ".length).trim();
  const tokens = tokenizeDiffHeaderPaths(body);
  const oldPath = trimDiffPath(tokens[0] ?? "");
  const newPath = trimDiffPath(tokens[1] ?? oldPath);
  return [oldPath, newPath];
}

// tokenizeDiffHeaderPaths splits a header path list while preserving quoted spaces.
// tokenizeDiffHeaderPaths 拆分头部路径列表，同时保留带引号路径中的空格。
function tokenizeDiffHeaderPaths(value: string): string[] {
  const tokens: string[] = [];
  const pattern = /"((?:\\.|[^"])*)"|(\S+)/g;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(value))) {
    tokens.push(match[1] ? unescapeQuotedPath(match[1]) : match[2]);
  }
  return tokens;
}

// trimDiffPath removes git's a/ and b/ prefixes for display.
// trimDiffPath 移除 git 用于展示的 a/ 与 b/ 前缀。
function trimDiffPath(path: string): string {
  const normalized = unescapeQuotedPath(path.trim().replace(/^"|"$/g, ""));
  return normalized.replace(/^[ab]\//, "");
}

// unescapeQuotedPath normalizes Git quoted-path escape sequences.
// unescapeQuotedPath 规范化 Git 引号路径转义序列。
function unescapeQuotedPath(path: string): string {
  const bytes: number[] = [];
  for (let index = 0; index < path.length;) {
    const character = path[index];
    if (character !== "\\") {
      const codePoint = path.codePointAt(index) ?? character.charCodeAt(0);
      const literal = String.fromCodePoint(codePoint);
      bytes.push(...textEncoder.encode(literal));
      index += literal.length;
      continue;
    }
    const escaped = path[index + 1];
    if (escaped === undefined) {
      bytes.push("\\".charCodeAt(0));
      index += 1;
      continue;
    }
    if (escaped >= "0" && escaped <= "7") {
      let value = 0;
      let offset = 1;
      while (offset <= 3) {
        const digit = path[index + offset];
        if (digit === undefined || digit < "0" || digit > "7") {
          break;
        }
        value = value * 8 + Number.parseInt(digit, 8);
        offset += 1;
      }
      bytes.push(Math.min(value, 0xff));
      index += offset;
      continue;
    }
    bytes.push(...gitQuotedEscapedCharacterBytes(escaped));
    index += 2;
  }
  return gitQuotedPathDecoder.decode(new Uint8Array(bytes));
}

// gitQuotedEscapedCharacterBytes returns UTF-8 bytes for one non-octal Git path escape.
// gitQuotedEscapedCharacterBytes 返回单个非八进制 Git 路径转义对应的 UTF-8 字节。
function gitQuotedEscapedCharacterBytes(
  // Escaped character following the backslash.
  // 反斜杠后的被转义字符。
  escaped: string,
): number[] {
  switch (escaped) {
    case "a":
      return [0x07];
    case "b":
      return [0x08];
    case "f":
      return [0x0c];
    case "n":
      return [0x0a];
    case "r":
      return [0x0d];
    case "t":
      return [0x09];
    case "v":
      return [0x0b];
    case "\\":
      return [0x5c];
    case "\"":
      return [0x22];
    default:
      return Array.from(textEncoder.encode(escaped));
  }
}

// lastWhitespaceSeparatedToken returns the final token without allocating split arrays.
// lastWhitespaceSeparatedToken 返回最后一个空白分隔 token，避免分配 split 数组。
function lastWhitespaceSeparatedToken(value: string): string {
  let end = value.length;
  while (end > 0 && isAsciiWhitespace(value.charCodeAt(end - 1))) {
    end -= 1;
  }
  let start = end;
  while (start > 0 && !isAsciiWhitespace(value.charCodeAt(start - 1))) {
    start -= 1;
  }
  return value.slice(start, end);
}

// isAsciiWhitespace checks Git metadata separators without regular-expression work.
// isAsciiWhitespace 检查 Git 元数据分隔空白，避免正则开销。
function isAsciiWhitespace(code: number): boolean {
  return code === 32 || code === 9 || code === 10 || code === 13 || code === 12;
}

// parseHunkHeader parses one unified hunk header.
// parseHunkHeader 解析单个 unified hunk 头部。
function parseHunkHeader(header: string): DiffHunk {
  const match = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(header);
  const oldStart = Number(match?.[1] ?? 0);
  const newStart = Number(match?.[2] ?? 0);
  return { header, oldStart, newStart, lines: [] };
}

// parseDiffLine converts one raw hunk row into a structured row.
// parseDiffLine 将单条原始 hunk 行转换为结构化行。
function parseDiffLine(rawLine: string, cursor: HunkCursor): DiffLine {
  if (rawLine.startsWith("+") && !rawLine.startsWith("+++")) {
    return { kind: "added", content: rawLine.slice(1), oldLine: null, newLine: cursor.newLine };
  }
  if (rawLine.startsWith("-") && !rawLine.startsWith("---")) {
    return { kind: "removed", content: rawLine.slice(1), oldLine: cursor.oldLine, newLine: null };
  }
  if (rawLine.startsWith(" ")) {
    return { kind: "context", content: rawLine.slice(1), oldLine: cursor.oldLine, newLine: cursor.newLine };
  }
  return { kind: "metadata", content: rawLine, oldLine: null, newLine: null };
}

// applyInlineChangeRanges precomputes paired-line ranges for cheap renderer consumption.
// applyInlineChangeRanges 为渲染器预计算配对行的行内变更范围。
function applyInlineChangeRanges(hunk: DiffHunk): void {
  let index = 0;
  while (index < hunk.lines.length) {
    if (hunk.lines[index].kind !== "removed") {
      index += 1;
      continue;
    }
    const removedStart = index;
    while (index < hunk.lines.length && hunk.lines[index].kind === "removed") {
      index += 1;
    }
    const addedStart = index;
    while (index < hunk.lines.length && hunk.lines[index].kind === "added") {
      index += 1;
    }
    if (addedStart === index) {
      continue;
    }
    applyInlineChangeRangesForRuns(hunk.lines, removedStart, addedStart, index);
  }
}

// applyInlineChangeRangesForRuns pairs adjacent removed and added runs with bounded similarity matching.
// applyInlineChangeRangesForRuns 使用有界相似度匹配配对相邻删除与新增块。
function applyInlineChangeRangesForRuns(lines: DiffLine[], removedStart: number, addedStart: number, addedEnd: number): void {
  for (const pair of inlineDiffLinePairsForRuns(lines, removedStart, addedStart, addedEnd)) {
    const removedLine = lines[pair.removedIndex];
    const addedLine = lines[pair.addedIndex];
    const ranges = inlineChangeRangesForPair(removedLine.content, addedLine.content);
    if (!ranges) {
      continue;
    }
    removedLine.inlineChangeRanges = ranges.removed;
    addedLine.inlineChangeRanges = ranges.added;
    removedLine.inlineChangeRange = ranges.removed.length === 1 ? ranges.removed[0] : undefined;
    addedLine.inlineChangeRange = ranges.added.length === 1 ? ranges.added[0] : undefined;
  }
}

// inlineDiffLinePairsForRuns selects stable removed/added row pairs inside one changed run.
// inlineDiffLinePairsForRuns 在单个变更块内选择稳定的删除/新增行配对。
function inlineDiffLinePairsForRuns(lines: readonly DiffLine[], removedStart: number, addedStart: number, addedEnd: number): InlineDiffLinePair[] {
  const removedCount = addedStart - removedStart;
  const addedCount = addedEnd - addedStart;
  if (removedCount <= 0 || addedCount <= 0) {
    return [];
  }
  if (removedCount * addedCount > INLINE_DIFF_MAX_RUN_PAIR_CELLS) {
    return inlineDiffLinePairsByRowOrder(removedStart, addedStart, removedCount, addedCount);
  }
  const similarityScores = inlineDiffLineSimilarityScores(lines, removedStart, addedStart, addedEnd);
  const pairs = inlineDiffLineLcsPairs(similarityScores, removedStart, addedStart);
  return pairs.length ? pairs : inlineDiffLinePairsByRowOrder(removedStart, addedStart, removedCount, addedCount);
}

// inlineDiffLinePairsByRowOrder returns the conservative adjacent-row pairing.
// inlineDiffLinePairsByRowOrder 返回保守的相邻行顺序配对。
function inlineDiffLinePairsByRowOrder(removedStart: number, addedStart: number, removedCount: number, addedCount: number): InlineDiffLinePair[] {
  const pairCount = Math.min(removedCount, addedCount);
  const pairs: InlineDiffLinePair[] = new Array(pairCount);
  for (let offset = 0; offset < pairCount; offset += 1) {
    pairs[offset] = {
      removedIndex: removedStart + offset,
      addedIndex: addedStart + offset,
    };
  }
  return pairs;
}

// inlineDiffLineSimilarityScores builds a bounded score matrix for changed-run line pairing.
// inlineDiffLineSimilarityScores 为变更块行配对构建有界相似度矩阵。
function inlineDiffLineSimilarityScores(
  lines: readonly DiffLine[],
  removedStart: number,
  addedStart: number,
  addedEnd: number,
): InlineDiffLineSimilarityMatrix {
  const removedCount = addedStart - removedStart;
  const addedCount = addedEnd - addedStart;
  const scores = new Uint16Array(removedCount * addedCount);
  for (let removedOffset = 0; removedOffset < removedCount; removedOffset += 1) {
    const removedContent = lines[removedStart + removedOffset].content;
    const rowOffset = removedOffset * addedCount;
    for (let addedOffset = 0; addedOffset < addedCount; addedOffset += 1) {
      scores[rowOffset + addedOffset] = inlineDiffLineSimilarityScore(removedContent, lines[addedStart + addedOffset].content);
    }
  }
  return { scores, removedCount, addedCount };
}

// inlineDiffLineLcsPairs computes an ordered maximum-score pairing from a line similarity matrix.
// inlineDiffLineLcsPairs 从行相似度矩阵计算有序的最高得分配对。
function inlineDiffLineLcsPairs(matrix: InlineDiffLineSimilarityMatrix, removedStart: number, addedStart: number): InlineDiffLinePair[] {
  const removedCount = matrix.removedCount;
  const addedCount = matrix.addedCount;
  const dpWidth = addedCount + 1;
  const dp = new Int32Array((removedCount + 1) * dpWidth);
  for (let removedOffset = 1; removedOffset <= removedCount; removedOffset += 1) {
    for (let addedOffset = 1; addedOffset <= addedCount; addedOffset += 1) {
      const score = matrix.scores[(removedOffset - 1) * addedCount + addedOffset - 1];
      const pairedScore = score >= INLINE_DIFF_MIN_LINE_SIMILARITY
        ? dp[(removedOffset - 1) * dpWidth + addedOffset - 1] + score
        : -1;
      const topScore = dp[(removedOffset - 1) * dpWidth + addedOffset];
      const leftScore = dp[removedOffset * dpWidth + addedOffset - 1];
      dp[removedOffset * dpWidth + addedOffset] = Math.max(topScore, leftScore, pairedScore);
    }
  }
  const pairs: InlineDiffLinePair[] = [];
  let removedOffset = removedCount;
  let addedOffset = addedCount;
  while (removedOffset > 0 && addedOffset > 0) {
    const score = matrix.scores[(removedOffset - 1) * addedCount + addedOffset - 1];
    const pairedScore = score >= INLINE_DIFF_MIN_LINE_SIMILARITY
      ? dp[(removedOffset - 1) * dpWidth + addedOffset - 1] + score
      : -1;
    if (pairedScore === dp[removedOffset * dpWidth + addedOffset]) {
      pairs.push({
        removedIndex: removedStart + removedOffset - 1,
        addedIndex: addedStart + addedOffset - 1,
      });
      removedOffset -= 1;
      addedOffset -= 1;
    } else if (dp[(removedOffset - 1) * dpWidth + addedOffset] >= dp[removedOffset * dpWidth + addedOffset - 1]) {
      removedOffset -= 1;
    } else {
      addedOffset -= 1;
    }
  }
  return pairs.reverse();
}

// inlineDiffLineSimilarityScore estimates whether two changed rows should receive inline diffing.
// inlineDiffLineSimilarityScore 估算两条变更行是否适合做行内 diff。
function inlineDiffLineSimilarityScore(removedContent: string, addedContent: string): number {
  if (removedContent === addedContent) {
    return 1000;
  }
  if (
    removedContent.length > INLINE_DIFF_MAX_LINE_LENGTH
    || addedContent.length > INLINE_DIFF_MAX_LINE_LENGTH
  ) {
    return 0;
  }
  const removedTokens = tokenizeInlineDiffContent(removedContent);
  const addedTokens = tokenizeInlineDiffContent(addedContent);
  if (!removedTokens.length || !addedTokens.length) {
    return 0;
  }
  if (
    removedTokens.length > INLINE_DIFF_MAX_TOKEN_COUNT
    || addedTokens.length > INLINE_DIFF_MAX_TOKEN_COUNT
  ) {
    return inlineDiffPrefixSuffixSimilarityScore(removedContent, addedContent);
  }
  const lcsLength = inlineDiffTokenLcsPairs(removedTokens, addedTokens).length;
  const normalizedLength = Math.max(removedTokens.length, addedTokens.length);
  return Math.round((lcsLength / normalizedLength) * 1000);
}

// inlineDiffPrefixSuffixSimilarityScore estimates line similarity without token DP.
// inlineDiffPrefixSuffixSimilarityScore 不使用 token DP 估算行相似度。
function inlineDiffPrefixSuffixSimilarityScore(removedContent: string, addedContent: string): number {
  const range = prefixSuffixInlineChangeRangesForPair(removedContent, addedContent);
  if (!range) {
    return 1000;
  }
  const removedChangedLength = range.removed.end - range.removed.start;
  const addedChangedLength = range.added.end - range.added.start;
  const stableLength = Math.max(0, Math.max(removedContent.length, addedContent.length) - Math.max(removedChangedLength, addedChangedLength));
  return Math.round((stableLength / Math.max(removedContent.length, addedContent.length, 1)) * 1000);
}

// inlineChangeRangesForPair returns token-LCS changed ranges for one removed/added pair.
// inlineChangeRangesForPair 返回一对删除/新增行的 token-LCS 变化范围。
function inlineChangeRangesForPair(removedContent: string, addedContent: string): { removed: DiffInlineChangeRange[]; added: DiffInlineChangeRange[] } | null {
  if (
    removedContent === addedContent
    || removedContent.length > INLINE_DIFF_MAX_LINE_LENGTH
    || addedContent.length > INLINE_DIFF_MAX_LINE_LENGTH
  ) {
    return null;
  }
  const removedTokens = tokenizeInlineDiffContent(removedContent);
  const addedTokens = tokenizeInlineDiffContent(addedContent);
  if (
    removedTokens.length > 0
    && addedTokens.length > 0
    && removedTokens.length <= INLINE_DIFF_MAX_TOKEN_COUNT
    && addedTokens.length <= INLINE_DIFF_MAX_TOKEN_COUNT
  ) {
    const pairs = inlineDiffTokenLcsPairs(removedTokens, addedTokens);
    const removedRanges = inlineChangeRangesBetweenTokenPairs(removedContent, removedTokens, pairs, "removed");
    const addedRanges = inlineChangeRangesBetweenTokenPairs(addedContent, addedTokens, pairs, "added");
    if (removedRanges.length || addedRanges.length) {
      return {
        removed: removedRanges,
        added: addedRanges,
      };
    }
  }
  const fallback = prefixSuffixInlineChangeRangesForPair(removedContent, addedContent);
  return fallback
    ? { removed: [fallback.removed], added: [fallback.added] }
    : null;
}

// prefixSuffixInlineChangeRangesForPair returns one bounded changed range as a fallback.
// prefixSuffixInlineChangeRangesForPair 返回一个受限的变化范围作为回退。
function prefixSuffixInlineChangeRangesForPair(removedContent: string, addedContent: string): { removed: DiffInlineChangeRange; added: DiffInlineChangeRange } | null {
  let prefixLength = 0;
  const shortestLength = Math.min(removedContent.length, addedContent.length);
  while (prefixLength < shortestLength && removedContent.charCodeAt(prefixLength) === addedContent.charCodeAt(prefixLength)) {
    prefixLength += 1;
  }
  let suffixLength = 0;
  while (
    suffixLength + prefixLength < removedContent.length
    && suffixLength + prefixLength < addedContent.length
    && removedContent.charCodeAt(removedContent.length - 1 - suffixLength) === addedContent.charCodeAt(addedContent.length - 1 - suffixLength)
  ) {
    suffixLength += 1;
  }
  const removedEnd = removedContent.length - suffixLength;
  const addedEnd = addedContent.length - suffixLength;
  if (removedEnd <= prefixLength && addedEnd <= prefixLength) {
    return null;
  }
  return {
    removed: { start: prefixLength, end: removedEnd },
    added: { start: prefixLength, end: addedEnd },
  };
}

// tokenizeInlineDiffContent splits one line into stable LCS tokens.
// tokenizeInlineDiffContent 将单行文本拆分为稳定的 LCS token。
function tokenizeInlineDiffContent(content: string): InlineDiffToken[] {
  const tokens: InlineDiffToken[] = [];
  let index = 0;
  while (index < content.length) {
    const start = index;
    const character = content[index];
    const kind = inlineDiffCharacterKind(character);
    if (kind === "punctuation") {
      tokens.push({ text: character, start, end: start + 1 });
      index += 1;
      continue;
    }
    index += 1;
    while (index < content.length && inlineDiffCharacterKind(content[index]) === kind) {
      index += 1;
    }
    tokens.push({ text: content.slice(start, index), start, end: index });
  }
  return tokens;
}

// inlineDiffCharacterKind groups characters for token-level LCS.
// inlineDiffCharacterKind 为 token 级 LCS 对字符分组。
function inlineDiffCharacterKind(character: string): "word" | "space" | "punctuation" {
  if (/\s/.test(character)) {
    return "space";
  }
  const code = character.charCodeAt(0);
  const asciiWord = (code >= 48 && code <= 57)
    || (code >= 65 && code <= 90)
    || (code >= 97 && code <= 122)
    || character === "_"
    || character === "$";
  if (asciiWord || code > 127) {
    return "word";
  }
  return "punctuation";
}

// inlineDiffTokenLcsPairs computes unchanged token pairs with a bounded dynamic-programming table.
// inlineDiffTokenLcsPairs 使用受限动态规划表计算未变化 token 对。
function inlineDiffTokenLcsPairs(removedTokens: readonly InlineDiffToken[], addedTokens: readonly InlineDiffToken[]): InlineDiffTokenPair[] {
  const addedWidth = addedTokens.length + 1;
  const table = new Uint16Array((removedTokens.length + 1) * addedWidth);
  for (let removedIndex = removedTokens.length - 1; removedIndex >= 0; removedIndex -= 1) {
    for (let addedIndex = addedTokens.length - 1; addedIndex >= 0; addedIndex -= 1) {
      const tableIndex = removedIndex * addedWidth + addedIndex;
      if (removedTokens[removedIndex].text === addedTokens[addedIndex].text) {
        table[tableIndex] = table[(removedIndex + 1) * addedWidth + addedIndex + 1] + 1;
      } else {
        table[tableIndex] = Math.max(
          table[(removedIndex + 1) * addedWidth + addedIndex],
          table[removedIndex * addedWidth + addedIndex + 1],
        );
      }
    }
  }
  const pairs: InlineDiffTokenPair[] = [];
  let removedIndex = 0;
  let addedIndex = 0;
  while (removedIndex < removedTokens.length && addedIndex < addedTokens.length) {
    if (removedTokens[removedIndex].text === addedTokens[addedIndex].text) {
      pairs.push({ removedIndex, addedIndex });
      removedIndex += 1;
      addedIndex += 1;
    } else if (table[(removedIndex + 1) * addedWidth + addedIndex] >= table[removedIndex * addedWidth + addedIndex + 1]) {
      removedIndex += 1;
    } else {
      addedIndex += 1;
    }
  }
  return pairs;
}

// inlineChangeRangesBetweenTokenPairs converts unchanged token pairs into merged changed character ranges.
// inlineChangeRangesBetweenTokenPairs 将未变化 token 对转换为合并后的变化字符范围。
function inlineChangeRangesBetweenTokenPairs(
  content: string,
  tokens: readonly InlineDiffToken[],
  pairs: readonly InlineDiffTokenPair[],
  side: "removed" | "added",
): DiffInlineChangeRange[] {
  const ranges: DiffInlineChangeRange[] = [];
  let previousEnd = 0;
  for (const pair of pairs) {
    const token = tokens[side === "removed" ? pair.removedIndex : pair.addedIndex];
    pushInlineChangeRange(ranges, previousEnd, token.start);
    previousEnd = token.end;
  }
  pushInlineChangeRange(ranges, previousEnd, content.length);
  mergeInlineChangeRangesAcrossWhitespace(ranges, content);
  return ranges;
}

// pushInlineChangeRange appends one non-empty range and merges touching ranges.
// pushInlineChangeRange 追加一个非空范围并合并相邻范围。
function pushInlineChangeRange(ranges: DiffInlineChangeRange[], start: number, end: number): void {
  if (end <= start) {
    return;
  }
  const previous = ranges[ranges.length - 1];
  if (previous && previous.end >= start) {
    previous.end = Math.max(previous.end, end);
    return;
  }
  ranges.push({ start, end });
}

// mergeInlineChangeRangesAcrossWhitespace makes adjacent word-level changes render as one visual block.
// mergeInlineChangeRangesAcrossWhitespace 让仅被空白隔开的相邻词级变更渲染为一个连续色块。
function mergeInlineChangeRangesAcrossWhitespace(ranges: DiffInlineChangeRange[], content: string): void {
  if (ranges.length <= 1) {
    return;
  }
  let writeIndex = 0;
  for (let readIndex = 1; readIndex < ranges.length; readIndex += 1) {
    const previous = ranges[writeIndex];
    const current = ranges[readIndex];
    if (isWhitespaceOnly(content, previous.end, current.start)) {
      previous.end = current.end;
      continue;
    }
    writeIndex += 1;
    ranges[writeIndex] = current;
  }
  ranges.length = writeIndex + 1;
}

// isWhitespaceOnly returns whether one content slice contains only spacing characters.
// isWhitespaceOnly 返回一段内容是否只包含空白字符。
function isWhitespaceOnly(content: string, start: number, end: number): boolean {
  for (let index = start; index < end; index += 1) {
    const code = content.charCodeAt(index);
    if (code !== 9 && code !== 10 && code !== 13 && code !== 32) {
      return false;
    }
  }
  return true;
}

// advanceCursor updates old and new line counters after one parsed row.
// advanceCursor 在解析单条行后推进旧文件与新文件行号。
function advanceCursor(cursor: HunkCursor, line: DiffLine): void {
  if (line.kind === "removed" || line.kind === "context") {
    cursor.oldLine += 1;
  }
  if (line.kind === "added" || line.kind === "context") {
    cursor.newLine += 1;
  }
}

// applyLineCounters appends one parsed row and updates file counters.
// applyLineCounters 追加单条解析行并更新文件计数。
function applyLineCounters(file: DiffFile, hunk: DiffHunk, line: DiffLine): void {
  if (line.kind === "added") {
    file.additions += 1;
    applyChangedLineMetrics(file, line.content);
  } else if (line.kind === "removed") {
    file.deletions += 1;
    applyChangedLineMetrics(file, line.content);
  }
  hunk.lines.push(line);
}

// applyChangedLineMetrics updates target-compatible changed-line counters.
// applyChangedLineMetrics 更新目标兼容的变更行计数。
function applyChangedLineMetrics(file: DiffFile, content: string): void {
  const lineBytes = textEncoder.encode(content).byteLength;
  file.changedLines += 1;
  file.changedBytes += lineBytes;
  file.maxChangedLineBytes = Math.max(file.maxChangedLineBytes, lineBytes);
}

// applyFileMode stores one parsed git file mode and detects gitlinks.
// applyFileMode 保存解析出的 git 文件模式并识别 gitlink。
function applyFileMode(file: DiffFile, mode: string): void {
  if (!mode) {
    return;
  }
  file.fileMode = mode;
  if (mode === "160000") {
    file.isGitlink = true;
  }
}

// applyIndexLineMode reads the optional mode suffix from an index metadata line.
// applyIndexLineMode 从 index 元数据行读取可选模式后缀。
function applyIndexLineMode(file: DiffFile, line: string): void {
  const mode = lastWhitespaceSeparatedToken(line);
  if (/^\d{6}$/.test(mode)) {
    applyFileMode(file, mode);
  }
}

// finalFileStatus resolves status after all file metadata and hunk rows are known.
// finalFileStatus 在所有文件元数据与 hunk 行已知后解析最终状态。
function finalFileStatus(file: DiffFile): DiffFile["status"] {
  file.largeReasons = diffFileLargeReasons(file);
  file.isLarge = file.largeReasons.length > 0;
  if (file.status === "added" || file.status === "deleted" || file.status === "renamed" || file.status === "binary") {
    return file.status;
  }
  if (file.oldPath === "/dev/null") {
    return "added";
  }
  if (file.newPath === "/dev/null") {
    return "deleted";
  }
  return "modified";
}
