import type { DiffFile, DiffHunk, DiffLine, DiffPanelSnapshot, DiffPanelSource } from "./types";

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

  // flushHunk commits the current hunk into the active file.
  // flushHunk 将当前 hunk 提交到活跃文件中。
  const flushHunk = () => {
    if (currentFile && currentHunk) {
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
      files.push(currentFile);
    }
    currentFile = null;
  };

  for (const rawLine of rawDiff.replace(/\r\n/g, "\n").split("\n")) {
    if (rawLine.startsWith("diff --git ")) {
      flushFile();
      const [oldPath, newPath] = parseDiffGitHeader(rawLine);
      currentFile = {
        key: `${oldPath}->${newPath}:${files.length}`,
        oldPath,
        newPath,
        status: "modified",
        additions: 0,
        deletions: 0,
        isBinary: false,
        hunks: [],
      };
      continue;
    }

    if (!currentFile) {
      continue;
    }

    if (rawLine.startsWith("new file mode")) {
      currentFile.status = "added";
      continue;
    }
    if (rawLine.startsWith("deleted file mode")) {
      currentFile.status = "deleted";
      continue;
    }
    if (rawLine.startsWith("rename from ")) {
      currentFile.oldPath = trimDiffPath(rawLine.slice("rename from ".length));
      currentFile.status = "renamed";
      continue;
    }
    if (rawLine.startsWith("rename to ")) {
      currentFile.newPath = trimDiffPath(rawLine.slice("rename to ".length));
      currentFile.status = "renamed";
      continue;
    }
    if (rawLine.startsWith("Binary files ") || rawLine.startsWith("GIT binary patch")) {
      currentFile.isBinary = true;
      currentFile.status = "binary";
      continue;
    }
    if (rawLine.startsWith("@@ ")) {
      flushHunk();
      currentHunk = parseHunkHeader(rawLine);
      currentCursor = { oldLine: currentHunk.oldStart, newLine: currentHunk.newStart };
      continue;
    }
    if (currentHunk && currentCursor) {
      const line = parseDiffLine(rawLine, currentCursor);
      applyLineCounters(currentFile, currentHunk, line);
      advanceCursor(currentCursor, line);
    }
  }

  flushFile();
  const stats = files.reduce(
    (acc, file) => ({
      files: acc.files + 1,
      additions: acc.additions + file.additions,
      deletions: acc.deletions + file.deletions,
    }),
    { files: 0, additions: 0, deletions: 0 },
  );
  return {
    source,
    repositoryRoot,
    files,
    stats,
    rawDiff,
    notice: files.length ? undefined : "当前没有可展示的 Git Diff。",
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

// unescapeQuotedPath normalizes simple git quoted-path escape sequences.
// unescapeQuotedPath 规范化简单的 git 引号路径转义序列。
function unescapeQuotedPath(path: string): string {
  return path.replace(/\\"/g, "\"").replace(/\\\\/g, "\\").replace(/\\t/g, "\t");
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
  } else if (line.kind === "removed") {
    file.deletions += 1;
  }
  hunk.lines.push(line);
}

// finalFileStatus resolves status after all file metadata and hunk rows are known.
// finalFileStatus 在所有文件元数据与 hunk 行已知后解析最终状态。
function finalFileStatus(file: DiffFile): DiffFile["status"] {
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
