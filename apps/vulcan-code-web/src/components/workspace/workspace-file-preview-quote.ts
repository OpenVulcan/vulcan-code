// Quoting from the file preview: a selection in the code preview becomes the whole source lines it touches,
// written under the file's path and line range so the model can find them in the project.
// 从文件预览引用：代码预览中的选区会扩展为其触及的完整源码行，并写在文件路径与行范围之下，便于模型在项目中定位。

// PREVIEW_CODE_CONTENT_SELECTOR names the part of a numbered preview row that holds the source text.
// PREVIEW_CODE_CONTENT_SELECTOR 指向带行号预览行中承载源码文本的部分。
const PREVIEW_CODE_CONTENT_SELECTOR = ".workspace-file-preview-code-content";

// coveredText returns the part of `content` that `range` covers.
// coveredText 返回 `range` 覆盖的 `content` 部分。
function coveredText(content: Element, range: Range): string {
  const part = content.ownerDocument.createRange();
  part.selectNodeContents(content);
  if (range.compareBoundaryPoints(Range.START_TO_START, part) > 0) part.setStart(range.startContainer, range.startOffset);
  if (range.compareBoundaryPoints(Range.END_TO_END, part) < 0) part.setEnd(range.endContainer, range.endOffset);
  return part.toString();
}

// previewSelectionLines returns the first and last line whose source text `range` covers, reading each row's
// data-line; a row the selection only touches at its edge does not count, nor does the line-number gutter.
// previewSelectionLines 返回 `range` 覆盖其源码文本的首行与末行（读取每行的 data-line）；只在边缘触及的行不计入，
// 行号栏也不计入。
export function previewSelectionLines(rows: Element, range: Range): { start: number; end: number } | null {
  let start = 0;
  let end = 0;
  for (const row of rows.children) {
    const line = Number((row as HTMLElement).dataset.line);
    const content = row.querySelector(PREVIEW_CODE_CONTENT_SELECTOR);
    if (!line || !content || !range.intersectsNode(content) || coveredText(content, range) === "") continue;
    if (!start) start = line;
    end = line;
  }
  return start ? { start, end } : null;
}

// selectPreviewLines selects the source text of lines `from` through `to` (in either order) in the numbered rows.
// selectPreviewLines 在带行号的各行中选中第 `from` 至 `to` 行（顺序不限）的源码文本。
export function selectPreviewLines(rows: Element, from: number, to: number): void {
  const first = rows.querySelector(`[data-line="${Math.min(from, to)}"] ${PREVIEW_CODE_CONTENT_SELECTOR}`);
  const last = rows.querySelector(`[data-line="${Math.max(from, to)}"] ${PREVIEW_CODE_CONTENT_SELECTOR}`);
  const selection = rows.ownerDocument.getSelection();
  if (!first || !last || !selection) return;
  const range = rows.ownerDocument.createRange();
  range.setStart(first, 0);
  range.setEnd(last, last.childNodes.length);
  selection.removeAllRanges();
  selection.addRange(range);
}

// codeQuoteMarkdown writes source lines as `@path:start-end` over a fenced block in the file's language, with
// a fence longer than any backtick run inside the code, and leaves an empty line for the user's own note.
// codeQuoteMarkdown 将源码行写为 `@path:start-end` 加上按文件语言标注的围栏代码块，围栏长于代码中任何反引号串，
// 并留一个空行供用户书写自己的说明。
export function codeQuoteMarkdown(path: string, lines: { start: number; end: number }, code: string, language: string | null): string {
  const longestTicks = Math.max(0, ...(code.match(/`+/gu) ?? []).map((run) => run.length));
  const fence = "`".repeat(Math.max(3, longestTicks + 1));
  const span = lines.start === lines.end ? `${lines.start}` : `${lines.start}-${lines.end}`;
  return `@${path.replace(/\\/gu, "/")}:${span}\n${fence}${language ?? ""}\n${code}\n${fence}\n\n`;
}
