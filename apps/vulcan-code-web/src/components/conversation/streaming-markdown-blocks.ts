// A growing tail block longer than this renders as plain text until the stream seals or the block closes.
// 增长中的尾块超过该长度时，在流封存或块闭合前以纯文本渲染。
export const STREAMING_MARKDOWN_RAW_TAIL_MAX_CHARS = 16 * 1024;

// FENCE_OPEN matches a fenced-code opening line at any indentation, so fences nested in list items stay whole.
// FENCE_OPEN 匹配任意缩进的围栏代码起始行，使嵌套在列表项中的围栏保持完整。
const FENCE_OPEN = /^\s*(`{3,}|~{3,})(.*)$/u;

// LIST_ITEM matches a bullet or ordered list marker that starts a line.
// LIST_ITEM 匹配位于行首的无序或有序列表标记。
const LIST_ITEM = /^([-*+]|\d{1,9}[.)])(\s|$)/u;

// splitStreamingMarkdownBlocks splits a growing Markdown body into top-level blocks that parse independently.
// splitStreamingMarkdownBlocks 将增长中的 Markdown 正文拆分为可独立解析的顶层块。
//
// # Parameters
// 参数
// `markdown` is the complete current body; the function keeps no state between calls.
// `markdown` 是当前完整正文；函数在调用之间不保留状态。
//
// # Returns
// 返回值
// Blocks in source order without their separating blank lines. A split happens only at a blank line outside
// fenced code whose next line cannot continue the previous block (indented content, list items after a list,
// quote lines after a quote), so every block renders as it would inside the full body. Link reference
// definitions and footnotes resolve across blocks only after the stream seals.
// 按源码顺序返回的块，不含分隔空行。仅在围栏代码之外、且下一行无法延续上一块（缩进内容、列表之后的列表项、
// 引用之后的引用行）的空行处拆分，因此每块的渲染结果都与在完整正文中一致。链接引用定义与脚注只在流封存后
// 才跨块解析。
export function splitStreamingMarkdownBlocks(markdown: string): string[] {
  const blocks: string[] = [];
  let current: string[] = [];
  let blankRun = 0;
  let fence: { marker: string; length: number } | null = null;
  for (const line of markdown.split(/\r?\n/u)) {
    if (fence) {
      current.push(line);
      const closing = line.trim();
      if (closing.length >= fence.length && closing === fence.marker.repeat(closing.length)) {
        fence = null;
      }
      continue;
    }
    if (line.trim() === "") {
      if (current.length > 0) {
        blankRun += 1;
      }
      continue;
    }
    if (blankRun > 0 && !continuesBlock(current[0], line)) {
      blocks.push(current.join("\n"));
      current = [];
    } else {
      // Blank lines inside a continued block are content (indented code, loose list items), so their count is kept.
      // 延续块内部的空行属于内容（缩进代码、松散列表项），因此保留其数量。
      for (let index = 0; index < blankRun; index += 1) {
        current.push("");
      }
    }
    blankRun = 0;
    current.push(line);
    fence = openingFence(line);
  }
  if (current.length > 0) {
    blocks.push(current.join("\n"));
  }
  return blocks;
}

// openingFence returns the fence a line opens, or null when the line opens none.
// openingFence 返回该行开启的围栏；未开启围栏时返回 null。
function openingFence(line: string): { marker: string; length: number } | null {
  const opening = FENCE_OPEN.exec(line);
  if (!opening) {
    return null;
  }
  // A backtick fence's info string cannot contain a backtick; such a line is inline code, not an opening fence.
  // 反引号围栏的信息串不能包含反引号；这样的行是行内代码而不是围栏起始。
  if (opening[1][0] === "`" && opening[2].includes("`")) {
    return null;
  }
  return { marker: opening[1][0], length: opening[1].length };
}

// continuesBlock reports whether a line after a blank run still belongs to the block that started with `first`.
// continuesBlock 判断空行之后的一行是否仍属于以 `first` 开头的块。
function continuesBlock(first: string, line: string): boolean {
  if (/^\s/u.test(line)) {
    return true;
  }
  if (LIST_ITEM.test(first) && LIST_ITEM.test(line)) {
    return true;
  }
  return first.startsWith(">") && line.startsWith(">");
}
