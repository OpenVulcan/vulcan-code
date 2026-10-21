// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { ConversationFindBar } from "./conversation-find-bar";

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Views stands in for the workbench: a transcript with the conversation bar and a file preview with a scoped bar.
// Views 代替工作台：带会话查找栏的转录，以及带限定范围查找栏的文件预览。
function Views(props: { onGotoLine?: (line: number) => void }) {
  const transcript = React.useRef<HTMLDivElement | null>(null);
  const code = React.useRef<HTMLDivElement | null>(null);
  return (
    <>
      <div className="vk-chat"><div data-testid="transcript" ref={transcript} tabIndex={0}>hello</div></div>
      <ConversationFindBar onReveal={() => undefined} scrollRef={transcript} />
      <div data-find-scope="">
        <div data-testid="code" ref={code} tabIndex={0}>hello</div>
        <ConversationFindBar highlight="vk-file-find" label="workspaceFiles.find" onGotoLine={props.onGotoLine} onReveal={() => undefined} scoped scrollRef={code} />
      </div>
    </>
  );
}

const openBars = () => [...document.querySelectorAll(".vk-find-bar")].map((bar) => (bar.closest("[data-find-scope]") ? "file" : "conversation"));

async function pressFind(target: Element, key = "f") {
  await act(async () => {
    (target as HTMLElement).focus();
    target.dispatchEvent(new KeyboardEvent("keydown", { key, ctrlKey: true, bubbles: true, cancelable: true }));
  });
}

it("gives Ctrl+F inside a find scope to that scope's bar alone, and Ctrl+F elsewhere to the conversation", async () => {
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><Views /></I18nProvider>));
  await pressFind(document.querySelector("[data-testid=code]")!);
  expect(openBars()).toEqual(["file"]);
  expect(document.activeElement?.closest("[data-find-scope]")).not.toBeNull();

  await pressFind(document.querySelector("[data-testid=transcript]")!);
  expect(openBars().sort()).toEqual(["conversation", "file"]);
});

it("does not open the scoped bar for Ctrl+F pressed outside its scope", async () => {
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><Views /></I18nProvider>));
  await pressFind(document.body);
  expect(openBars()).toEqual(["conversation"]);
});

it("opens a bar that can go to lines on Ctrl+G with the caret after ':', and goes to the typed line on Enter", async () => {
  const onGotoLine = vi.fn();
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><Views onGotoLine={onGotoLine} /></I18nProvider>));
  await pressFind(document.querySelector("[data-testid=transcript]")!, "g");
  expect(openBars()).toEqual([]);

  await pressFind(document.querySelector("[data-testid=code]")!, "g");
  const input = document.activeElement as HTMLInputElement;
  expect(openBars()).toEqual(["file"]);
  expect([input.value, input.selectionStart, input.selectionEnd]).toEqual([":", 1, 1]);

  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, ":42");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => { input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
  expect(onGotoLine.mock.calls).toEqual([[42]]);
  expect(openBars()).toEqual([]);
  expect(document.activeElement?.getAttribute("data-testid")).toBe("code");
});
