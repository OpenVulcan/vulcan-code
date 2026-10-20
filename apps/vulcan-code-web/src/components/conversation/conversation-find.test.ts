// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";
import { CONVERSATION_FIND_MATCH_LIMIT, findTextRanges } from "./conversation-find";

afterEach(() => { document.body.replaceChildren(); });

// mount places transcript markup in the document and returns its root.
// mount 将转录标记放入文档并返回其根元素。
function mount(html: string): HTMLElement {
  const root = document.createElement("section");
  root.innerHTML = html;
  document.body.append(root);
  return root;
}

it("finds every occurrence regardless of case, including words split across inline elements", () => {
  const root = mount("<p>Alpha beta</p><p>al<strong>pha</strong> and ALPHA</p>");
  expect(findTextRanges(root, "alpha").map((range) => range.toString())).toEqual(["Alpha", "alpha", "ALPHA"]);
});

it("skips decoration and hidden text, and treats a blank query as no search", () => {
  const root = mount('<p>needle</p><span aria-hidden="true">needle</span><p hidden>needle</p><p data-find-skip>needle</p>');
  expect(findTextRanges(root, "needle")).toHaveLength(1);
  expect(findTextRanges(root, "   ")).toEqual([]);
});

it("stops at the match limit", () => {
  const root = mount(`<p>${"x".repeat(CONVERSATION_FIND_MATCH_LIMIT + 5)}</p>`);
  expect(findTextRanges(root, "x")).toHaveLength(CONVERSATION_FIND_MATCH_LIMIT);
});
