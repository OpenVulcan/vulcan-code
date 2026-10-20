// Turns one persisted change-set file into display rows and counts. Context rows get line numbers derived from the
// changed lines they surround, so every row shows where it sits in the old and new file.
// 将一条持久化变更集文件转换为展示行与计数。上下文行的行号由其包围的变更行推导，使每行都能显示在新旧文件中的位置。
import type { PersistentChangeSetFile, PersistentLifecycleContent } from "@/service/conversation-event-window-store";

// DiffRow is one rendered diff line; omit rows stand for elided text.
// DiffRow 是一行渲染的差异；omit 行代表被省略的文本。
export interface DiffRow {
  // Row kind.
  // 行类型。
  kind: "context" | "insert" | "delete" | "omit";
  // Old-side line number, when the row exists there.
  // 旧侧行号（该行存在于旧侧时）。
  oldLine: number | null;
  // New-side line number, when the row exists there.
  // 新侧行号（该行存在于新侧时）。
  newLine: number | null;
  // Line text, or the elided line count for omit rows.
  // 行文本；omit 行为省略的行数。
  text: string;
  // Lines elided by an omit row.
  // omit 行省略的行数。
  omitted?: number;
}

// fileDisplayPath returns the path a change-set file is known by after the change.
// fileDisplayPath 返回变更后用于识别该变更集文件的路径。
export function fileDisplayPath(file: PersistentChangeSetFile): string {
  return file.new_path ?? file.path ?? file.old_path ?? "";
}

// fileLineCounts returns lines added and removed, or null when the producer gave no text facts.
// fileLineCounts 返回新增与删除行数；产出方未提供文本事实时返回 null。
export function fileLineCounts(file: PersistentChangeSetFile): { added: number; removed: number } | null {
  if (file.hunks.length > 0) {
    return file.hunks.reduce(
      (total, hunk) => ({ added: total.added + hunk.inserted.length, removed: total.removed + hunk.deleted.length }),
      { added: 0, removed: 0 },
    );
  }
  const total = file.lifecycle_content?.total_line_count;
  if (total === undefined) return null;
  if (file.change === "create") return { added: total, removed: 0 };
  if (file.change === "delete") return { added: 0, removed: total };
  return { added: 0, removed: 0 };
}

// fileDiffRows returns the rows to draw for one file.
// fileDiffRows 返回一个文件需要绘制的行。
export function fileDiffRows(file: PersistentChangeSetFile): DiffRow[] {
  if (file.hunks.length > 0) return hunkRows(file);
  return file.lifecycle_content ? lifecycleRows(file.change, file.lifecycle_content) : [];
}

// hunkRows walks hunks in order, tracking the new-minus-old offset to number context on both sides.
// hunkRows 按顺序遍历 hunk，跟踪新旧偏移量以便为两侧上下文编号。
function hunkRows(file: PersistentChangeSetFile): DiffRow[] {
  const rows: DiffRow[] = [];
  let offset = 0;
  for (const hunk of file.hunks) {
    const firstDeleted = hunk.deleted[0]?.line;
    const firstInserted = hunk.inserted[0]?.line;
    const oldStart = firstDeleted ?? (firstInserted !== undefined ? firstInserted - offset : 1);
    const newStart = firstInserted ?? oldStart + offset;
    const before = textLines(hunk.before);
    before.forEach((text, index) => {
      rows.push({ kind: "context", oldLine: oldStart - before.length + index, newLine: newStart - before.length + index, text });
    });
    for (const line of hunk.deleted) rows.push({ kind: "delete", oldLine: line.line, newLine: null, text: line.content });
    for (const line of hunk.inserted) rows.push({ kind: "insert", oldLine: null, newLine: line.line, text: line.content });
    const oldAfter = oldStart + hunk.deleted.length;
    const newAfter = newStart + hunk.inserted.length;
    textLines(hunk.after).forEach((text, index) => {
      rows.push({ kind: "context", oldLine: oldAfter + index, newLine: newAfter + index, text });
    });
    offset += hunk.inserted.length - hunk.deleted.length;
  }
  return rows;
}

// lifecycleRows draws created or deleted file text, eliding what the producer left out.
// lifecycleRows 绘制新建或删除文件的正文，并省略产出方未保留的部分。
function lifecycleRows(change: PersistentChangeSetFile["change"], content: PersistentLifecycleContent): DiffRow[] {
  const kind: DiffRow["kind"] = change === "delete" ? "delete" : change === "create" ? "insert" : "context";
  const numbered = (lines: string[], start: number): DiffRow[] => lines.map((text, index) => ({
    kind,
    newLine: kind === "delete" ? null : start + index,
    oldLine: kind === "insert" ? null : start + index,
    text,
  }));
  if (content.mode === "full") return numbered(textLines(content.content), 1);
  if (content.mode === "omitted") {
    return [{ kind: "omit", newLine: null, oldLine: null, omitted: content.total_line_count, text: "" }];
  }
  const head = textLines(content.head);
  const tail = textLines(content.tail);
  const omitted = Math.max(0, content.total_line_count - head.length - tail.length);
  return [
    ...numbered(head, 1),
    { kind: "omit", newLine: null, oldLine: null, omitted, text: "" },
    ...numbered(tail, content.total_line_count - tail.length + 1),
  ];
}

// textLines splits text into logical lines, ignoring one trailing newline.
// textLines 将文本拆分为逻辑行，忽略末尾的一个换行。
export function textLines(text: string): string[] {
  const normalized = text.replace(/\r\n?/g, "\n");
  if (normalized.length === 0) return [];
  const lines = normalized.split("\n");
  if (normalized.endsWith("\n")) lines.pop();
  return lines;
}
