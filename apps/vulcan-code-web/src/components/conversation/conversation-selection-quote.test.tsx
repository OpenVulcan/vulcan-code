// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ConversationSelectionQuote, quoteAsMarkdown } from "./conversation-selection-quote";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));

// host, root, timelineRef and scrollRef own one transcript with a neighbouring composer per test.
// host、root、timelineRef 与 scrollRef 为每个测试持有一份会话转录及相邻输入区。
let host: HTMLDivElement;
let root: Root;
let timelineRef: React.RefObject<HTMLDivElement | null>;
let scrollRef: React.RefObject<HTMLElement | null>;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // jsdom has no layout, so every range and viewport reports one fixed visible box.
  // jsdom 没有布局，因此每个选区与视口都报告同一个固定的可见矩形。
  Object.defineProperty(Range.prototype, "getBoundingClientRect", {
    configurable: true,
    value: () => new DOMRect(100, 200, 80, 20),
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 800, 600));
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  timelineRef = React.createRef<HTMLDivElement>();
  scrollRef = React.createRef<HTMLElement>();
});

afterEach(async () => {
  await act(async () => root.unmount());
  window.getSelection()?.removeAllRanges();
  host.remove();
  Reflect.deleteProperty(Range.prototype, "getBoundingClientRect");
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

// mountQuote renders the transcript and a composer-shaped node outside its selection scope.
// mountQuote 渲染会话转录及一个位于选区范围外的输入区形态节点。
async function mountQuote(onQuote: (text: string) => void): Promise<void> {
  await act(async () => root.render(
    <section ref={scrollRef}>
      <ConversationSelectionQuote timelineRef={timelineRef} scrollRef={scrollRef} onQuote={onQuote} />
      <div ref={timelineRef}><p data-line="first">First line</p><p>Second line</p></div>
      <div data-composer>Composer draft</div>
    </section>,
  ));
}

// selectAndRelease selects the node's contents and ends the gesture the way a pointer release does.
// selectAndRelease 选中节点内容，并像松开指针那样结束手势。
async function selectAndRelease(node: Node): Promise<void> {
  const range = document.createRange();
  range.selectNodeContents(node);
  window.getSelection()!.removeAllRanges();
  window.getSelection()!.addRange(range);
  await act(async () => {
    document.dispatchEvent(new Event("pointerup"));
    await new Promise((resolve) => window.requestAnimationFrame(() => resolve(null)));
  });
}

// quoteButton finds the floating action by its label key.
// quoteButton 按文案键查找悬浮操作。
function quoteButton(): HTMLButtonElement | undefined {
  return [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "conversation.contextMenu.quote");
}

it("offers the selected transcript text to the composer and then dismisses itself", async () => {
  const quote = vi.fn((_text: string) => {});
  await mountQuote(quote);
  await selectAndRelease(timelineRef.current!.querySelector('[data-line="first"]')!);

  expect(quoteButton()).toBeDefined();
  await act(async () => quoteButton()!.click());
  expect(quote).toHaveBeenCalledExactlyOnceWith("First line");
  expect(quoteButton()).toBeUndefined();
  expect(window.getSelection()?.isCollapsed ?? true).toBe(true);
});

it("ignores selections that reach outside the transcript", async () => {
  await mountQuote(vi.fn());
  await selectAndRelease(document.querySelector("[data-composer]")!);
  expect(quoteButton()).toBeUndefined();
});

it("formats every selected line as one block quote followed by an empty line", () => {
  expect(quoteAsMarkdown("\nalpha\r\n\r\nbeta  \n")).toBe("> alpha\n>\n> beta\n\n");
});
