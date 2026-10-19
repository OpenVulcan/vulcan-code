// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ComposerAnswerReplySlot } from "@/components/composer/composer-panel/answer-reply-slot";
import { MarkdownMessage } from "../conversation-markdown";
import { AnswerReplyContext, AnswerReplyStore } from "./answer-reply";
import { formatAnswerReply } from "./answer-reply-tray";

// Only the text catalog is synthetic; keys stand in for localized labels.
// 仅文案目录为合成边界；以键代替本地化标签。
vi.mock("@/i18n", async (importOriginal) => ({
  ...await importOriginal<typeof import("@/i18n")>(),
  useI18n: () => ({ t: (key: string) => key }),
}));

const DECISION = "```ui-decision\nCache lifetime?\n* 10 minutes | Matches deploys\n- 1 hour\n```";
const MULTI = "```ui-decision multi\nAlso cache?\n* Sessions\n- API responses\n- Fragments\n```";
const FINDINGS = "```ui-findings\nhigh | Unbounded map | src/cache.ts:3\n```";

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// mount renders a reply with an optional reply store and waits until the deferred components have drawn `ready`.
// mount 以可选回复存储渲染回答，并等待延迟组件绘制出 `ready`。
async function mount(content: string, store: AnswerReplyStore | null, ready: string) {
  await act(async () => root.render(
    <AnswerReplyContext.Provider value={store}><MarkdownMessage content={content} eager /></AnswerReplyContext.Provider>,
  ));
  await drawn(ready);
}

// drawn waits until a deferred element matching `ready` is in the document.
// drawn 等待匹配 `ready` 的延迟元素出现在文档中。
async function drawn(ready: string) {
  // The first mount transforms the deferred chunk, which can take seconds under a loaded parallel run.
  // 首次挂载需要转换延迟分块，在高负载的并行运行中可能耗时数秒。
  for (const deadline = Date.now() + 5000; Date.now() < deadline && !host.querySelector(ready);) {
    await act(async () => new Promise((resolve) => setTimeout(resolve, 10)));
  }
  expect(host.querySelector(ready), `${ready} never rendered`).not.toBeNull();
}

// options returns the option buttons of the decision at `index`.
// options 返回第 `index` 个决策的选项按钮。
function options(index = 0): HTMLButtonElement[] {
  return [...host.querySelectorAll(".answer-decision")[index].querySelectorAll<HTMLButtonElement>(".answer-decision-option")];
}

// type sets a textarea value the way React observes user input.
// type 以 React 能观察到的用户输入方式设置文本框的值。
async function type(textarea: HTMLTextAreaElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  await act(async () => {
    setter.call(textarea, value);
    textarea.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("answer replies", () => {
  it("records a single choice, toggles it off and keeps the recommendation", async () => {
    const store = new AnswerReplyStore();
    await mount(DECISION, store, ".answer-decision");
    expect(options().map((option) => option.getAttribute("role"))).toEqual(["radio", "radio"]);
    expect(options()[0].hasAttribute("data-recommended")).toBe(true);
    await act(async () => options()[1].click());
    expect(store.list()).toEqual([{ kind: "decision", question: "Cache lifetime?", picks: ["1 hour"], recommended: ["10 minutes"], note: "" }]);
    expect(options()[1].getAttribute("aria-checked")).toBe("true");
    await act(async () => options()[0].click());
    expect(store.list()[0]).toMatchObject({ picks: ["10 minutes"] });
    await act(async () => options()[0].click());
    expect(store.size()).toBe(0);
  });

  it("keeps multiple picks in option order", async () => {
    const store = new AnswerReplyStore();
    await mount(MULTI, store, ".answer-decision");
    expect(options()[0].getAttribute("role")).toBe("checkbox");
    await act(async () => options()[2].click());
    await act(async () => options()[0].click());
    expect(store.list()[0]).toMatchObject({ picks: ["Sessions", "Fragments"] });
  });

  it("shows decisions read-only where no reply can be composed", async () => {
    await mount(DECISION, null, ".answer-decision");
    expect(options().every((option) => option.disabled)).toBe(true);
    expect(host.querySelector(".answer-decision [aria-label='answerComponents.comment']")).toBeNull();
  });

  it("comments on a framed component and closes the editor once the reply is taken", async () => {
    const store = new AnswerReplyStore();
    await mount(FINDINGS, store, ".answer-findings, .vk-figure");
    const button = host.querySelector<HTMLButtonElement>(".vk-figure-hover [aria-label='answerComponents.comment']");
    expect(button).not.toBeNull();
    await act(async () => button!.click());
    await type(host.querySelector<HTMLTextAreaElement>(".answer-note textarea")!, "Cap it first");
    expect(store.list()).toEqual([{ kind: "comment", subject: expect.stringMatching(/^ui-findings: high \| Unbounded map/u), note: "Cap it first" }]);
    await act(async () => store.clear());
    expect(host.querySelector(".answer-note")).toBeNull();
  });

  it("appends the reply to the composer draft and empties the store, never replacing earlier text", async () => {
    const store = new AnswerReplyStore();
    let draft = "Earlier text";
    const setDraftInput: React.Dispatch<React.SetStateAction<string>> = (update) => {
      draft = typeof update === "function" ? update(draft) : update;
    };
    await act(async () => root.render(
      <AnswerReplyContext.Provider value={store}>
        <ComposerAnswerReplySlot disabled={false} setDraftInput={setDraftInput} textareaRef={{ current: null }} />
      </AnswerReplyContext.Provider>,
    ));
    expect(host.querySelector(".answer-reply-tray")).toBeNull();
    await act(async () => store.set("fence", { kind: "comment", subject: "ui-kv: a | b", note: "Check" }));
    await drawn(".answer-reply-tray");
    await act(async () => host.querySelector<HTMLButtonElement>(".answer-reply-tray-insert")!.click());
    expect(draft.split("\n")).toEqual(["Earlier text", "", "**answerComponents.reply.comments**", "- ui-kv: a | b", "  > Check"]);
    expect(store.size()).toBe(0);
    expect(host.querySelector(".answer-reply-tray")).toBeNull();
  });

  it("formats decisions before comments with notes as quotes", () => {
    const text = formatAnswerReply([
      { kind: "comment", subject: "ui-steps: done | Build", note: "Flag first\nthen async" },
      { kind: "decision", question: "Q1?", picks: ["A", "B"], recommended: ["A"], note: "" },
      { kind: "decision", question: "Q2?", picks: [], recommended: [], note: "Unsure" },
    ], (key) => key);
    expect(text.split("\n")).toEqual([
      "**answerComponents.reply.decisions**",
      "- Q1? → A (answerComponents.decision.recommended), B",
      "- Q2? → answerComponents.reply.noChoice",
      "  > Unsure",
      "",
      "**answerComponents.reply.comments**",
      "- ui-steps: done | Build",
      "  > Flag first",
      "  > then async",
    ]);
  });
});
