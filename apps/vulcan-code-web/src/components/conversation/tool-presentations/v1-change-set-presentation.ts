// Builds canonical file-change views from persisted ChangeSet records for the diff cards.
// 从持久化 ChangeSet 记录构建规范文件变更视图，供差异卡片使用。
import { formatMessage, translateActive } from "@/i18n"
import type {
  PersistentChangeSetFile,
  PersistentLifecycleContent,
  PersistentTextFormat,
} from "@/service/conversation-event-window-store";

/** One line rendered by a persisted ChangeSet file body.
 * 持久化 ChangeSet 文件正文渲染的一行。 */
export interface V1ChangeSetRow {
  kind: "context" | "added" | "deleted" | "omitted";
  lineNumber: number | null;
  content: string;
  logicalLineCount: number;
}

/** One file view derived exclusively from a persisted ChangeSet file record.
 * 完全从持久化 ChangeSet 文件记录派生的单文件视图。 */
export interface V1ChangeSetFilePresentation {
  resourceId: string | null;
  change: PersistentChangeSetFile["change"];
  // Canonical path used for stable identity and list keys.
  // 用于稳定身份与列表键的 canonical 路径。
  path: string;
  // Exact persisted rename source path when the operation is a rename.
  // 操作为重命名时持久化的精确源路径。
  oldPath: string | null;
  // Exact persisted rename destination path when the operation is a rename.
  // 操作为重命名时持久化的精确目标路径。
  newPath: string | null;
  // Stable user-visible path, including both sides of a rename when available.
  // 稳定的用户可见路径；重命名两端可用时同时展示。
  displayPath: string;
  additions: number;
  deletions: number;
  rows: readonly V1ChangeSetRow[];
  beforeFormat: string | null;
  afterFormat: string | null;
}

/** Derives one file presentation from exact persisted lifecycle and hunk data.
 * 从精确持久化生命周期与 hunk 数据派生单文件展示。 */
export function buildV1ChangeSetFilePresentation(
  file: PersistentChangeSetFile,
): V1ChangeSetFilePresentation {
  // rows preserve producer order and carry all changed-line accounting.
  // rows 保留产出方顺序并携带全部变更行统计。
  const rows = changeSetFileRows(file);
  // path remains the canonical identity while displayPath carries rename semantics.
  // path 保持 canonical 身份，displayPath 则携带重命名语义。
  const path = changeSetFilePath(file);
  // oldPath and newPath are exact persisted facts and never inferred from the filesystem.
  // oldPath 与 newPath 是精确持久化事实，绝不从文件系统推断。
  const oldPath = file.old_path ?? null;
  const newPath = file.new_path ?? null;
  return {
    resourceId: file.resource_id ?? null,
    change: file.change,
    path,
    oldPath,
    newPath,
    displayPath: file.change === "rename" && oldPath && newPath
      ? `${oldPath} → ${newPath}`
      : path,
    additions: rows.reduce((total, row) => total + (row.kind === "added" || (row.kind === "omitted" && file.change === "create") ? row.logicalLineCount : 0), 0),
    deletions: rows.reduce((total, row) => total + (row.kind === "deleted" || (row.kind === "omitted" && file.change === "delete") ? row.logicalLineCount : 0), 0),
    rows,
    beforeFormat: persistentTextFormatLabel(file.before_text_format),
    afterFormat: persistentTextFormatLabel(file.after_text_format),
  };
}

/** Selects the authoritative row source for each persisted file-change kind.
 * 为每种持久化文件变更类型选择权威行来源。 */
function changeSetFileRows(file: PersistentChangeSetFile): V1ChangeSetRow[] {
  if (file.change === "modify") {
    return hunkRows(file);
  }
  if (file.change === "rename") {
    // Rename hunks own content-change accounting; lifecycle text is only the pure-rename body fallback.
    // rename hunks 拥有内容变更统计；生命周期正文只作为纯重命名的正文来源。
    return file.hunks.length > 0
      ? hunkRows(file)
      : file.lifecycle_content
        ? lifecycleRows(file.change, file.lifecycle_content)
        : [];
  }
  return file.lifecycle_content
    ? lifecycleRows(file.change, file.lifecycle_content)
    : hunkRows(file);
}

/** Converts exact modify hunks to display rows without sorting or merging them.
 * 在不排序或合并的情况下将精确 modify hunks 转为展示行。 */
function hunkRows(file: PersistentChangeSetFile): V1ChangeSetRow[] {
  return file.hunks.flatMap((hunk) => [
    ...contextRows(hunk.before),
    ...hunk.deleted.map((line) => ({
      kind: "deleted" as const,
      lineNumber: line.line,
      content: line.content,
      logicalLineCount: 1,
    })),
    ...hunk.inserted.map((line) => ({
      kind: "added" as const,
      lineNumber: line.line,
      content: line.content,
      logicalLineCount: 1,
    })),
    ...contextRows(hunk.after),
  ]);
}

/** Converts stable hunk context text to non-counted display rows.
 * 将稳定 hunk 上下文文本转换为不计入变更统计的展示行。 */
function contextRows(text: string): V1ChangeSetRow[] {
  return logicalLines(text).map((content) => ({
    kind: "context",
    lineNumber: null,
    content,
    logicalLineCount: 0,
  }));
}

/** Builds lifecycle rows directly from full, truncated, or omitted producer semantics.
 * 直接按照产出方的 full、truncated 或 omitted 语义构建生命周期行。 */
function lifecycleRows(
  change: PersistentChangeSetFile["change"],
  lifecycle: PersistentLifecycleContent,
): V1ChangeSetRow[] {
  // rowKind follows the only lifecycle operations that own +/- totals.
  // rowKind 遵循唯一拥有加减统计的生命周期操作。
  const rowKind = change === "delete" ? "deleted" : change === "create" ? "added" : "context";
  if (lifecycle.mode === "omitted") {
    return [{
      kind: "omitted",
      lineNumber: null,
      content: translateActive("toolDomain.changeSet.omittedByProducer"),
      logicalLineCount: lifecycle.total_line_count ?? 0,
    }];
  }
  if (lifecycle.mode === "full") {
    // retainedRows are the exact inline body supplied by the producer.
    // retainedRows 是产出方提供的精确内联正文。
    return numberedRows(logicalLines(lifecycle.content), rowKind, 1);
  }
  // headLines and tailLines remain separate so tail line numbers use the complete total.
  // headLines 与 tailLines 保持分离，使尾部行号使用完整总行数。
  const headLines = logicalLines(lifecycle.head);
  const tailLines = logicalLines(lifecycle.tail);
  const retainedCount = headLines.length + tailLines.length;
  const completeCount = lifecycle.total_line_count;
  const omittedCount = completeCount - retainedCount;
  return [
    ...numberedRows(headLines, rowKind, 1),
    ...(omittedCount > 0 ? [{
      kind: "omitted" as const,
      lineNumber: null,
      content: formatMessage(translateActive("toolDomain.changeSet.omittedLines"), { count: omittedCount }),
      logicalLineCount: omittedCount,
    }] : []),
    ...numberedRows(tailLines, rowKind, completeCount - tailLines.length + 1),
  ];
}

/** Assigns producer-order line numbers to one retained lifecycle text fragment.
 * 为一段保留的生命周期正文按产出方顺序分配行号。 */
function numberedRows(
  lines: readonly string[],
  kind: V1ChangeSetRow["kind"],
  startLine: number,
): V1ChangeSetRow[] {
  return lines.map((content, index) => ({
    kind,
    lineNumber: startLine + index,
    content,
    logicalLineCount: kind === "context" ? 0 : 1,
  }));
}

/** Splits persisted text into logical lines without inventing a trailing empty line.
 * 将持久化文本拆成逻辑行，且不虚构末尾空行。 */
function logicalLines(text: string): string[] {
  if (!text) {
    return [];
  }
  // lines preserves empty interior lines while removing only a final terminator artifact.
  // lines 保留内部空行，仅移除末尾换行产生的分割伪项。
  const lines = text.split(/\r\n|\n|\r/);
  if (lines.at(-1) === "") {
    lines.pop();
  }
  return lines;
}

/** Selects the declared path for each change kind without filesystem probing.
 * 在不探测文件系统的情况下为每种变更类型选择声明路径。 */
function changeSetFilePath(file: PersistentChangeSetFile): string {
  if (file.change === "rename") {
    return file.new_path ?? file.old_path ?? file.path ?? file.resource_id ?? "unknown";
  }
  return file.path ?? file.new_path ?? file.old_path ?? file.resource_id ?? "unknown";
}

/** Formats one persisted text-format snapshot into stable inline metadata.
 * 将一份持久化文本格式快照格式化为稳定行内元数据。 */
export function persistentTextFormatLabel(format: PersistentTextFormat | undefined): string | null {
  if (!format) {
    return null;
  }
  // parts contains only values explicitly persisted by the producer.
  // parts 仅包含产出方明确持久化的值。
  const parts = [format.encoding, format.line_ending].filter((value): value is string => Boolean(value));
  if (format.bom === true) {
    parts.push("BOM");
  }
  if (format.mixed_line_endings === true) {
    parts.push("mixed");
  }
  return parts.length > 0 ? parts.join(" · ") : null;
}
