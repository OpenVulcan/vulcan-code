// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AssistantStep } from "./assistant-conversation";
import type { AssistantReplyStep } from "./assistant-last-turn";

// Only the text catalog is synthetic; keys stand in for localized labels.
// 仅文案目录为合成边界；以键代替本地化标签。
vi.mock("@/i18n", async (importOriginal) => ({
  ...await importOriginal<typeof import("@/i18n")>(),
  useI18n: () => ({ t: (key: string) => key }),
}));

// jsdom has no layout, so every box reports a fixed size and keeps the scroll offsets it is given.
// jsdom 没有布局，因此每个盒子报告固定尺寸，并保留被设置的滚动偏移。
const LAYOUT = { scrollWidth: 900, clientWidth: 300, scrollHeight: 800, clientHeight: 192 };
const offsets = new WeakMap<Element, { scrollLeft: number; scrollTop: number }>();
const descriptors = new Map<string, PropertyDescriptor | undefined>();

// offset returns the stored scroll offsets of `element`.
// offset 返回 `element` 已保存的滚动偏移。
function offset(element: Element) {
  let value = offsets.get(element);
  if (!value) offsets.set(element, value = { scrollLeft: 0, scrollTop: 0 });
  return value;
}

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  for (const [name, size] of Object.entries(LAYOUT)) {
    descriptors.set(name, Object.getOwnPropertyDescriptor(Element.prototype, name));
    Object.defineProperty(Element.prototype, name, { configurable: true, get: () => size });
  }
  for (const name of ["scrollLeft", "scrollTop"] as const) {
    descriptors.set(name, Object.getOwnPropertyDescriptor(Element.prototype, name));
    Object.defineProperty(Element.prototype, name, {
      configurable: true,
      get(this: Element) { return offset(this)[name]; },
      set(this: Element, value: number) { offset(this)[name] = value; },
    });
  }
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  for (const [name, descriptor] of descriptors) {
    if (descriptor) Object.defineProperty(Element.prototype, name, descriptor);
    else Reflect.deleteProperty(Element.prototype, name);
  }
  descriptors.clear();
  vi.unstubAllGlobals();
});

// step builds a reply step whose thinking is `reasoning` and whose answer is `answer`.
// step 构造思考为 `reasoning`、回答为 `answer` 的回复步骤。
function step(reasoning: { text: string; streaming: boolean } | null, answer: { text: string; streaming: boolean } | null = null): AssistantReplyStep {
  return { apiCallId: 1, reasoning, answer, tools: [], usage: null, durationMs: null };
}

async function render(value: AssistantReplyStep) {
  await act(async () => root.render(<AssistantStep step={value} />));
}

const ticker = () => host.querySelector<HTMLElement>(".assistant-thinking-ticker");
const thinking = () => host.querySelector<HTMLElement>(".assistant-thinking-text");
const toggle = () => host.querySelector<HTMLButtonElement>("[data-slot='inline-disclosure-trigger']")!;

describe("assistant thinking", () => {
  it("stays folded and streams as one line that only moves toward the newest text", async () => {
    await render(step({ text: "first\nthought", streaming: true }));
    expect(thinking()).toBeNull();
    expect(ticker()!.textContent).toBe("first thought");
    expect(ticker()!.scrollLeft).toBe(LAYOUT.scrollWidth);
    expect(ticker()!.hasAttribute("data-overflow")).toBe(true);
    ticker()!.scrollLeft = 0;
    await render(step({ text: "first\nthought and more", streaming: true }));
    expect(ticker()!.scrollLeft).toBe(LAYOUT.scrollWidth);
  });

  it("drops the line once thinking ends or the answer starts", async () => {
    await render(step({ text: "done", streaming: false }));
    expect(ticker()).toBeNull();
    await render(step({ text: "still", streaming: true }, { text: "Answer", streaming: true }));
    expect(ticker()).toBeNull();
  });

  it("opens pinned to the bottom, follows new text, and stops following once the reader scrolls up", async () => {
    await render(step({ text: "a", streaming: true }));
    await act(async () => toggle().click());
    expect(ticker()).toBeNull();
    expect(thinking()!.scrollTop).toBe(LAYOUT.scrollHeight);
    thinking()!.scrollTop = 0;
    await render(step({ text: "ab", streaming: true }));
    expect(thinking()!.scrollTop).toBe(LAYOUT.scrollHeight);
    thinking()!.scrollTop = 100;
    await act(async () => thinking()!.dispatchEvent(new Event("scroll")));
    await render(step({ text: "abc", streaming: true }));
    expect(thinking()!.scrollTop).toBe(100);
  });
});
