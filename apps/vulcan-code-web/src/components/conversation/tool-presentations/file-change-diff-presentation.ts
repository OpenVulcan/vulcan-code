import type { DiffFile, DiffLine } from "@/diff/types";
import type { FileChangeDiffRow, FileChangeOperation } from "./types";
import { arrayValue, compactHash, numberValue, objectValue, stringValue, textValue } from "./presentation-value-readers";

// fileChangeDiffFile adapts change-set rows into the shared review diff renderer model.
// fileChangeDiffFile 将 change-set 行适配为共享代码审核 diff 渲染模型。
export function fileChangeDiffFile(args: {
  // Added line count for this file.
  // 当前文件的新增行数。
  added: number | null;
  // File index inside the change-set payload.
  // 文件在变更集载荷中的序号。
  index: number;
  // Normalized operation for this file.
  // 当前文件的规范化操作。
  operation: FileChangeOperation;
  // Display path.
  // 展示路径。
  path: string;
  // Removed line count for this file.
  // 当前文件的删除行数。
  removed: number | null;
  // Render rows for this file.
  // 当前文件的渲染行。
  rows: FileChangeDiffRow[];
}): DiffFile | null {
  const lines = fileChangeDiffLines(args.rows);
  if (!lines.length) {
    return null;
  }
  const additions = args.added ?? lines.filter((line) => line.kind === "added").length;
  const deletions = args.removed ?? lines.filter((line) => line.kind === "removed").length;
  const oldStart = firstLineNumber(lines, "oldLine") ?? 1;
  const newStart = firstLineNumber(lines, "newLine") ?? 1;
  return {
    additions,
    changedBytes: changedLineBytes(lines),
    changedLines: additions + deletions,
    deletions,
    hunks: [{
      header: `@@ -${oldStart},${Math.max(1, deletions)} +${newStart},${Math.max(1, additions)} @@`,
      lines,
      newStart,
      oldStart,
    }],
    isBinary: args.rows.some((row) => row.kind === "binary"),
    isGitlink: false,
    isLarge: false,
    key: `change-set:${args.index}:${compactHash(args.path)}`,
    largeReasons: [],
    maxChangedLineBytes: maxChangedLineBytes(lines),
    newPath: args.operation === "delete" ? "/dev/null" : args.path,
    oldPath: args.operation === "create" ? "/dev/null" : args.path,
    rowCount: lines.length + 1,
    status: diffFileStatusForOperation(args.operation),
  };
}

// insertRows converts created content into green inserted rows.
// insertRows 将创建内容转换为绿色插入行。
export function insertRows(lines: string[]): FileChangeDiffRow[] {
  return lines.map((line, index) => ({
    content: line,
    kind: "insert",
    lineNumber: index + 1,
  }));
}

// modifyRows converts hunk payloads into context, insert, and delete rows.
// modifyRows 将 hunk 载荷转换为上下文、插入与删除行。
export function modifyRows(hunks: unknown[]): FileChangeDiffRow[] {
  const rows: FileChangeDiffRow[] = [];
  for (const rawHunk of hunks) {
    const hunk = objectValue(rawHunk);
    if (!hunk) {
      continue;
    }
    const deleted = lineEntries(hunk.delete, "delete");
    const inserted = lineEntries(hunk.insert, "insert");
    const firstChangedLine = deleted[0]?.lineNumber ?? inserted[0]?.lineNumber ?? 1;
    const beforeLines = splitTextLines(textValue(hunk.before) ?? "");
    const beforeStart = Math.max(1, firstChangedLine - beforeLines.length);
    rows.push(...beforeLines.map((line, index) => ({
      content: line,
      kind: "context" as const,
      lineNumber: beforeStart + index,
    })));
    rows.push(...deleted);
    rows.push(...inserted);
    const afterLines = splitTextLines(textValue(hunk.after) ?? "");
    const lastChangedLine = inserted.at(-1)?.lineNumber ?? deleted.at(-1)?.lineNumber ?? firstChangedLine;
    rows.push(...afterLines.map((line, index) => ({
      content: line,
      kind: "context" as const,
      lineNumber: lastChangedLine + index + 1,
    })));
  }
  return rows;
}

// deleteRows converts delete payloads into red removed rows with truncated separators.
// deleteRows 将删除载荷转换为带截断分隔符的红色删除行。
export function deleteRows(file: Record<string, unknown>): FileChangeDiffRow[] {
  const mode = stringValue(file.content_mode ?? file.contentMode);
  const content = textValue(file.content);
  if (isBinaryFileContent(content)) {
    return [{ content: "Binary file", kind: "binary", lineNumber: null }];
  }
  if (mode === "truncated") {
    const total = numberValue(file.total_line_count ?? file.totalLineCount) ?? 0;
    const head = splitTextLines(textValue(file.content_head ?? file.contentHead) ?? "");
    const tail = splitTextLines(textValue(file.content_tail ?? file.contentTail) ?? "");
    const omitted = Math.max(0, total - head.length - tail.length);
    const tailStart = Math.max(head.length + 1, total - tail.length + 1);
    return [
      ...head.map((line, index) => ({ content: line, kind: "delete" as const, lineNumber: index + 1 })),
      ...(omitted > 0 ? [{ content: `${omitted} lines omitted`, kind: "omit" as const, lineNumber: null }] : []),
      ...tail.map((line, index) => ({ content: line, kind: "delete" as const, lineNumber: tailStart + index })),
    ];
  }
  return splitTextLines(content ?? "").map((line, index) => ({
    content: line,
    kind: "delete",
    lineNumber: index + 1,
  }));
}

// splitTextLines mirrors LuaSkills text line handling for display counts.
// splitTextLines 镜像 LuaSkills 文本行处理以保持展示计数一致。
export function splitTextLines(source: string): string[] {
  if (!source) {
    return [];
  }
  const normalized = source.replace(/\r\n|\n\r|\r|\n/g, "\n");
  const withoutFinalNewline = normalized.endsWith("\n") ? normalized.slice(0, -1) : normalized;
  return withoutFinalNewline ? withoutFinalNewline.split("\n") : [];
}

// fileChangeDiffLines converts display rows into shared parsed diff lines.
// fileChangeDiffLines 将展示行转换为共享解析 diff 行。
function fileChangeDiffLines(rows: FileChangeDiffRow[]): DiffLine[] {
  return rows
    .filter((row) => row.kind !== "error")
    .map((row, index): DiffLine => {
      if (row.kind === "insert") {
        return {
          content: row.content,
          kind: "added",
          newLine: row.lineNumber ?? index + 1,
          oldLine: null,
        };
      }
      if (row.kind === "delete") {
        return {
          content: row.content,
          kind: "removed",
          newLine: null,
          oldLine: row.lineNumber ?? index + 1,
        };
      }
      return {
        content: row.content,
        kind: row.kind === "omit" || row.kind === "binary" ? "metadata" : "context",
        newLine: row.kind === "omit" || row.kind === "binary" ? null : row.lineNumber,
        oldLine: row.kind === "omit" || row.kind === "binary" ? null : row.lineNumber,
      };
    });
}

// diffFileStatusForOperation maps file-change operations to review diff statuses.
// diffFileStatusForOperation 将文件变更操作映射为代码审核 diff 状态。
function diffFileStatusForOperation(operation: FileChangeOperation): DiffFile["status"] {
  if (operation === "create") return "added";
  if (operation === "delete") return "deleted";
  return "modified";
}

// firstLineNumber returns the first concrete old or new line number.
// firstLineNumber 返回首个具体旧侧或新侧行号。
function firstLineNumber(lines: DiffLine[], key: "oldLine" | "newLine"): number | null {
  for (const line of lines) {
    const value = line[key];
    if (typeof value === "number") {
      return value;
    }
  }
  return null;
}

// changedLineBytes estimates changed text bytes for lightweight diff metadata.
// changedLineBytes 估算轻量 diff 元数据中的变更文本字节数。
function changedLineBytes(lines: DiffLine[]): number {
  let total = 0;
  for (const line of lines) {
    if (line.kind === "added" || line.kind === "removed") {
      total += textByteLength(line.content);
    }
  }
  return total;
}

// maxChangedLineBytes estimates the largest changed row byte size.
// maxChangedLineBytes 估算最大变更行字节数。
function maxChangedLineBytes(lines: DiffLine[]): number {
  let max = 0;
  for (const line of lines) {
    if (line.kind === "added" || line.kind === "removed") {
      max = Math.max(max, textByteLength(line.content));
    }
  }
  return max;
}

// textByteLength returns a UTF-8 byte estimate for diff rendering limits.
// textByteLength 返回用于 diff 渲染限制的 UTF-8 字节估算。
function textByteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

// lineEntries converts LuaSkills line entry arrays into diff rows.
// lineEntries 将 LuaSkills 行条目数组转换为 diff 行。
function lineEntries(value: unknown, kind: "insert" | "delete"): FileChangeDiffRow[] {
  return arrayValue(value).map((entry) => {
    const item = objectValue(entry);
    const lineNumber = numberValue(item?.line);
    const content = textValue(item?.content) ?? "";
    return {
      content,
      kind,
      lineNumber,
    };
  });
}

// isBinaryFileContent detects the stable binary placeholder returned by vulcan-file delete.
// isBinaryFileContent 检测 vulcan-file delete 返回的稳定二进制占位。
function isBinaryFileContent(content: string | null): boolean {
  return content?.trim() === "Binary file";
}
