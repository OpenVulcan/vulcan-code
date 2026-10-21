import { describe, expect, it } from "vitest";

import { memorySearchPages } from "@/conversation/tools/cards/memory-search-card";

// RESULT mirrors the VMM result renderer: query groups, memory headings in rank order, then bodies.
// RESULT 镜像 VMM 结果渲染器：查询分组、按排名排列的记忆标题及其正文。
const RESULT = [
  "# Memory search",
  "",
  "## Query 1: `deploy steps`",
  "",
  "### Memory 7",
  "",
  "Source turn: 2 · Category: `fact`",
  "",
  "First summary",
  "",
  "## Query 2: `` uses `code` ``",
  "",
  "No matches.",
  "",
  "## Query 3: `colors`",
  "",
  "### Memory 12",
  "",
  "Second summary",
].join("\r\n");

describe("memorySearchPages", () => {
  // Every memory becomes one page carrying its own query, id and body.
  // 每条记忆成为一页，携带其查询、标识与正文。
  it("splits memories into pages with their query", () => {
    const pages = memorySearchPages(RESULT);
    expect(pages.map((page) => [page.query, page.memoryId])).toEqual([
      ["deploy steps", "7"],
      ["colors", "12"],
    ]);
    expect(pages[0].body).toContain("First summary");
    expect(pages[0].body).not.toContain("Query 2");
    expect(pages[1].body).toBe("Second summary");
  });

  // A result without memory headings yields no pages, so the card shows its empty state.
  // 没有记忆标题的结果不产生页面，卡片显示空状态。
  it("returns no pages when nothing matched", () => {
    expect(memorySearchPages("# Memory search\n\n## Query 1: `x`\n\nNo matches.\n")).toEqual([]);
  });
});
