// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useComposerTextareaAutosize } from "./use-composer-textarea-autosize";

// jsdom has no layout: scrollHeight and clientWidth are synthetic, and ResizeObserver is captured to fire by hand.
// jsdom 没有布局：scrollHeight 与 clientWidth 为合成值，ResizeObserver 被捕获以便手动触发。
let host: HTMLDivElement;
let root: Root;
let resize: (() => void) | null = null;
let layout = { scrollHeight: 60, clientWidth: 400 };
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) { resize = callback; }
    observe() {}
    disconnect() { resize = null; }
  });
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockImplementation(() => layout.scrollHeight);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(() => layout.clientWidth);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

// Harness mounts one textarea driven by the hook; value is the draft text.
// Harness 挂载一个由该 hook 驱动的输入框；value 为草稿文本。
function Harness({ value }: { value: string }) {
  const ref = React.useRef<HTMLTextAreaElement | null>(null);
  useComposerTextareaAutosize(ref, value);
  return <textarea ref={ref} value={value} readOnly />;
}

const field = () => host.querySelector("textarea")!;

it("sets the height from the content each time the text changes", async () => {
  await act(async () => root.render(<Harness value="one" />));
  expect(field().style.height).toBe("60px");
  layout = { ...layout, scrollHeight: 180 };
  await act(async () => root.render(<Harness value={"one\ntwo\nthree\nfour"} />));
  expect(field().style.height).toBe("180px");
});

it("measures again when the width changes but not when only the height changes", async () => {
  layout = { scrollHeight: 60, clientWidth: 400 };
  await act(async () => root.render(<Harness value="long line" />));
  layout = { scrollHeight: 120, clientWidth: 400 };
  await act(async () => resize?.());
  expect(field().style.height).toBe("60px");
  layout = { scrollHeight: 120, clientWidth: 200 };
  await act(async () => resize?.());
  expect(field().style.height).toBe("120px");
});
