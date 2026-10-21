// @vitest-environment jsdom
import { expect, it } from "vitest";
import { codeQuoteMarkdown, previewSelectionLines, selectPreviewLines } from "./workspace-file-preview-quote";

// previewRows builds numbered preview rows shaped like the code preview's, one per source line.
// previewRows 构建与代码预览形状一致的带行号预览行，每个源码行一行。
function previewRows(lines: string[]): HTMLElement {
  const container = document.createElement("div");
  lines.forEach((text, index) => {
    const row = document.createElement("div");
    row.dataset.line = String(index + 1);
    const gutter = document.createElement("span");
    gutter.className = "workspace-file-preview-code-gutter";
    gutter.textContent = String(index + 1);
    const content = document.createElement("span");
    content.className = "workspace-file-preview-code-content";
    content.textContent = text || " ";
    row.append(gutter, content);
    container.append(row);
  });
  document.body.append(container);
  return container;
}

// contentText returns the text node holding one row's source text.
// contentText 返回承载某行源码文本的文本节点。
function contentText(container: HTMLElement, line: number): Text {
  return container.children[line - 1].querySelector(".workspace-file-preview-code-content")!.firstChild as Text;
}

it("widens a selection to the source lines whose text it covers", () => {
  const container = previewRows(["alpha", "beta", "gamma", "delta"]);
  const range = document.createRange();
  range.setStart(contentText(container, 2), 2);
  range.setEnd(contentText(container, 3), 1);
  expect(previewSelectionLines(container, range)).toEqual({ start: 2, end: 3 });
});

it("leaves out rows the selection only touches at an edge or in the gutter", () => {
  const container = previewRows(["alpha", "beta", "gamma"]);
  const range = document.createRange();
  // From the end of line 1 to the start of line 3's text, as a drag past a line end does.
  // 从第 1 行末尾到第 3 行文本开头，如同拖过行尾时产生的选区。
  range.setStart(contentText(container, 1), 5);
  range.setEnd(contentText(container, 3), 0);
  expect(previewSelectionLines(container, range)).toEqual({ start: 2, end: 2 });
  const gutterOnly = document.createRange();
  gutterOnly.selectNodeContents(container.children[0].firstChild!);
  expect(previewSelectionLines(container, gutterOnly)).toBeNull();
});

it("writes the path with forward slashes, the line span and a fence longer than the code's backticks", () => {
  expect(codeQuoteMarkdown("src\\main.ts", { start: 4, end: 4 }, "let a = 1;", "ts")).toBe("@src/main.ts:4\n```ts\nlet a = 1;\n```\n\n");
  const quoted = codeQuoteMarkdown("README.md", { start: 2, end: 5 }, "```js\nx\n```", null);
  expect(quoted.split("\n").slice(0, 2)).toEqual(["@README.md:2-5", "````"]);
  expect(quoted.endsWith("\n````\n\n")).toBe(true);
});

it("selects whole source lines in either order, which map back to the same line span", () => {
  const container = previewRows(["alpha", "beta", "gamma", "delta"]);
  selectPreviewLines(container, 3, 2);
  const selection = document.getSelection()!;
  expect(selection.toString()).toMatch(/^beta.*gamma$/);
  expect(previewSelectionLines(container, selection.getRangeAt(0))).toEqual({ start: 2, end: 3 });
});
