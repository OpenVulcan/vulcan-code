// Conversation find: locates text the reader can currently see in a transcript and marks it with the CSS Custom
// Highlight API, so the transcript DOM is never rewritten. Text inside collapsed groups, hidden elements and
// aria-hidden decoration is skipped, and only the loaded part of the history is searched.
// 会话查找：在转录中定位读者当前可见的文本，并通过 CSS 自定义高亮 API 标记，从不改写转录 DOM。折叠组、隐藏元素与
// aria-hidden 装饰内的文本会被跳过，且只搜索已加载的历史部分。

// CONVERSATION_FIND_MATCH_LIMIT bounds one search so a one-letter query on a long transcript stays cheap.
// CONVERSATION_FIND_MATCH_LIMIT 限制单次搜索的匹配数，使长转录上的单字母查询仍然开销低。
export const CONVERSATION_FIND_MATCH_LIMIT = 1000;

// CONVERSATION_FIND_HIGHLIGHT names the highlight for every match; the current match also gets the _CURRENT one.
// CONVERSATION_FIND_HIGHLIGHT 是所有匹配使用的高亮名称；当前匹配另外使用 _CURRENT 高亮。
export const CONVERSATION_FIND_HIGHLIGHT = "vk-find";
export const CONVERSATION_FIND_HIGHLIGHT_CURRENT = "vk-find-current";

// TextSpan is one searchable text node and where its text starts in the joined transcript text.
// TextSpan 是一个可搜索文本节点及其文本在拼接后转录文本中的起始位置。
interface TextSpan {
  node: Text;
  start: number;
}

// searchableText reports whether a text node is something the reader can see.
// searchableText 判断文本节点是否为读者可见的内容。
function searchableText(node: Text): boolean {
  const parent = node.parentElement;
  if (!parent || !node.data) return false;
  if (parent.closest("script, style, template, [aria-hidden='true'], [hidden], [data-find-skip]")) return false;
  // jsdom has no checkVisibility, where every element counts as visible.
  // jsdom 没有 checkVisibility，此时所有元素都视为可见。
  return typeof parent.checkVisibility !== "function" || parent.checkVisibility();
}

// foldCase lowers a text for matching while keeping its length, so offsets still point into the original text.
// foldCase 为匹配而转为小写并保持长度，使偏移仍指向原文本。
function foldCase(text: string): string {
  const lowered = text.toLowerCase();
  return lowered.length === text.length ? lowered : text;
}

// findTextRanges returns, in document order, a range for each case-insensitive occurrence of query in the visible
// text under root, stopping at CONVERSATION_FIND_MATCH_LIMIT. Matches may cross element boundaries, such as a word
// split by inline formatting or syntax highlighting.
// findTextRanges 按文档顺序返回 root 下可见文本中 query 每次不区分大小写出现的范围，达到
// CONVERSATION_FIND_MATCH_LIMIT 即停止。匹配可以跨越元素边界，例如被行内格式或语法高亮拆开的词。
export function findTextRanges(root: Element, query: string): Range[] {
  const needle = foldCase(query.trim());
  if (!needle) return [];
  const spans: TextSpan[] = [];
  let joined = "";
  const walker = root.ownerDocument.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = node as Text;
    if (!searchableText(text)) continue;
    spans.push({ node: text, start: joined.length });
    joined += foldCase(text.data);
  }
  const ranges: Range[] = [];
  let spanIndex = 0;
  // locate maps an offset in the joined text to its text node, scanning forward because matches arrive in order.
  // locate 将拼接文本中的偏移映射到其文本节点；匹配按顺序到达，因此只需向前扫描。
  const locate = (offset: number, end: boolean): { node: Text; offset: number } => {
    while (spanIndex + 1 < spans.length && (end ? spans[spanIndex + 1].start < offset : spans[spanIndex + 1].start <= offset)) {
      spanIndex += 1;
    }
    const span = spans[spanIndex];
    return { node: span.node, offset: offset - span.start };
  };
  for (let found = joined.indexOf(needle); found !== -1 && ranges.length < CONVERSATION_FIND_MATCH_LIMIT; found = joined.indexOf(needle, found + needle.length)) {
    const start = locate(found, false);
    const end = locate(found + needle.length, true);
    const range = root.ownerDocument.createRange();
    range.setStart(start.node, start.offset);
    range.setEnd(end.node, end.offset);
    ranges.push(range);
  }
  return ranges;
}

// paintConversationFind marks every match and the current one; it does nothing where the highlight API is missing.
// paintConversationFind 标记所有匹配与当前匹配；不支持高亮 API 时不做任何事。
export function paintConversationFind(ranges: readonly Range[], current: number): void {
  if (typeof CSS === "undefined" || !("highlights" in CSS) || typeof Highlight === "undefined") return;
  CSS.highlights.set(CONVERSATION_FIND_HIGHLIGHT, new Highlight(...ranges));
  const active = ranges[current];
  if (active) CSS.highlights.set(CONVERSATION_FIND_HIGHLIGHT_CURRENT, new Highlight(active));
  else CSS.highlights.delete(CONVERSATION_FIND_HIGHLIGHT_CURRENT);
}

// clearConversationFind removes both highlights.
// clearConversationFind 移除两种高亮。
export function clearConversationFind(): void {
  if (typeof CSS === "undefined" || !("highlights" in CSS)) return;
  CSS.highlights.delete(CONVERSATION_FIND_HIGHLIGHT);
  CSS.highlights.delete(CONVERSATION_FIND_HIGHLIGHT_CURRENT);
}
