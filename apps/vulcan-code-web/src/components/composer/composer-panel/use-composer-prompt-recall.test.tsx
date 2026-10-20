// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { clearSentPrompts, recordSentPrompt, sentPrompts, usePromptRecall } from "./use-composer-prompt-recall";

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => { callback(0); return 0; });
  clearSentPrompts();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); clearSentPrompts(); });

// Harness is a controlled textarea wired the way the composer wires it.
// Harness 是按输入区方式接线的受控文本框。
function Harness(props: { scope: string; initial?: string }) {
  const [input, setInput] = React.useState(props.initial ?? "");
  const textareaRef = React.useRef<HTMLTextAreaElement | null>(null);
  const recall = usePromptRecall({ scope: props.scope, slotKey: "slot", input, setInput, textareaRef });
  return <textarea ref={textareaRef} value={input} onChange={(event) => setInput(event.currentTarget.value)} onKeyDown={(event) => { recall(event); }} />;
}

// press sends one arrow key to the textarea and reports whether recall consumed it.
// press 向文本框发送一个方向键，并报告召回是否消费了它。
async function press(key: "ArrowUp" | "ArrowDown"): Promise<boolean> {
  const textarea = host.querySelector("textarea")!;
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  await act(async () => { textarea.dispatchEvent(event); });
  return event.defaultPrevented;
}

const value = () => host.querySelector("textarea")!.value;

it("walks back through the project's sent prompts and forward to the empty draft", async () => {
  recordSentPrompt("p1", "first");
  recordSentPrompt("p1", "second");
  recordSentPrompt("p2", "elsewhere");
  await act(async () => root.render(<Harness scope="p1" />));
  expect(await press("ArrowUp")).toBe(true);
  expect(value()).toBe("second");
  await press("ArrowUp");
  expect(value()).toBe("first");
  expect(await press("ArrowUp")).toBe(false);
  expect(value()).toBe("first");
  await press("ArrowDown");
  expect(value()).toBe("second");
  await press("ArrowDown");
  expect(value()).toBe("");
  expect(await press("ArrowDown")).toBe(false);
});

it("leaves typed text and caret movement between lines alone", async () => {
  recordSentPrompt("p1", "sent");
  await act(async () => root.render(<Harness scope="p1" initial="typed" />));
  expect(await press("ArrowUp")).toBe(false);
  expect(value()).toBe("typed");
  recordSentPrompt("p1", "line one\nline two");
  await act(async () => root.render(<Harness key="fresh" scope="p1" />));
  await press("ArrowUp");
  expect(value()).toBe("line one\nline two");
  // The caret sits at the end, on the last line, so ArrowUp moves it up instead of recalling.
  // 光标位于末尾的最后一行，因此 ArrowUp 移动光标而不是召回。
  expect(await press("ArrowUp")).toBe(false);
  host.querySelector("textarea")!.setSelectionRange(0, 0);
  expect(await press("ArrowUp")).toBe(true);
  expect(value()).toBe("sent");
});

it("keeps one copy of a repeated prompt at the newest place", () => {
  recordSentPrompt("p1", "again");
  recordSentPrompt("p1", "other");
  recordSentPrompt("p1", "again");
  recordSentPrompt("p1", "   ");
  expect(sentPrompts("p1")).toEqual(["other", "again"]);
});

