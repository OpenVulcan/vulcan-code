// @vitest-environment jsdom
// Verify mutually exclusive payload tabs in jsdom.
// 使用 jsdom 验证互斥的载荷标签页。
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ModelExtensionToolPayloadCards } from "./conversation-model-extension-tool-payloads";
import { I18nProvider } from "@/i18n";

// host owns the mounted payload cards for each test.
// host 持有每个测试挂载的载荷卡片。
let host: HTMLDivElement;
// root owns the React lifecycle for the mounted payload cards.
// root 持有已挂载载荷卡片的 React 生命周期。
let root: Root;

beforeEach(() => {
  // React act environment flag enables deterministic hook updates in jsdom.
  // React act 环境标记在 jsdom 中启用确定性的 Hook 更新。
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: () => ({
      addEventListener: () => undefined,
      matches: true,
      media: "(prefers-reduced-motion: reduce)",
      onchange: null,
      removeEventListener: () => undefined,
    }),
  });
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  delete (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
});

describe("ModelExtensionToolPayloadCards", () => {
  // Output is the default tab; switching panels never renders both payload bodies together.
  // 输出是默认标签页；切换面板时不会同时展示两份载荷正文。
  it("defaults to output and switches exclusively to input", async () => {
    await act(async () => {
      root.render(
        <I18nProvider initialLocale="en-US">
          <ModelExtensionToolPayloadCards
            inputMarkdown={'```json\n{"prompt":"hello"}\n```'}
            outputMarkdown={'```json\n{"task_id":"task-1"}\n```'}
            presentationId="extension-call-1"
          />
        </I18nProvider>,
      );
    });
    // inputButton is the exact control owned by the input payload card.
    // inputButton 是输入载荷卡持有的精确控件。
    const inputButton = host.querySelector<HTMLButtonElement>('[role="tab"][data-payload-tab="input"]')!;
    // outputButton is the exact control owned by the output payload card.
    // outputButton 是返回载荷卡持有的精确控件。
    const outputButton = host.querySelector<HTMLButtonElement>('[role="tab"][data-payload-tab="output"]')!;
    expect(inputButton.getAttribute("aria-selected")).toBe("false");
    expect(outputButton.getAttribute("aria-selected")).toBe("true");
    expect(Array.from(host.querySelectorAll("[data-model-extension-payload]"),
      (card) => card.getAttribute("data-model-extension-payload"))).toEqual(["output", "input"]);
    expect(inputButton?.querySelector(".lucide-log-in")).not.toBeNull();
    expect(outputButton?.querySelector(".lucide-log-out")).not.toBeNull();

    expect(host.querySelector('[role="tabpanel"]:not([hidden])')?.textContent).toContain("task-1");
    await act(async () => inputButton.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 })));
    expect(inputButton.getAttribute("aria-selected")).toBe("true");
    expect(outputButton.getAttribute("aria-selected")).toBe("false");
    expect(host.querySelector('[role="tabpanel"]:not([hidden])')?.textContent).toContain("hello");
    expect(host.querySelectorAll('[role="tabpanel"]:not([hidden])')).toHaveLength(1);

    await act(async () => outputButton.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Enter" })));
    expect(inputButton.getAttribute("aria-selected")).toBe("false");
    expect(outputButton.getAttribute("aria-selected")).toBe("true");
    expect(host.querySelector('[role="tabpanel"]:not([hidden])')?.textContent).toContain("task-1");
  });

  // Deferred output becomes the default unless the reader explicitly selected input.
  // 延迟输出到达后默认显示输出，但保留阅读者显式选择的输入页。
  it.each([false, true])("handles output arrival with explicit input selection %s", async (selectInput) => {
    // renderPayloads updates only synthetic content on the same mounted tool identity.
    // renderPayloads 仅更新相同挂载工具身份中的合成内容。
    const renderPayloads = (output: string) => <I18nProvider initialLocale="en-US">
      <ModelExtensionToolPayloadCards presentationId="pending" inputMarkdown="request" outputMarkdown={output} />
    </I18nProvider>;
    await act(async () => root.render(renderPayloads("")));
    expect(host.querySelectorAll('[role="tab"]')).toHaveLength(1);
    expect(host.querySelector('[role="tabpanel"]:not([hidden])')?.textContent).toBe("request");
    if (selectInput) {
      await act(async () => host.querySelector('[role="tab"]')!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 })));
    }
    await act(async () => root.render(renderPayloads("response")));
    expect(host.querySelector('[role="tabpanel"]:not([hidden])')?.textContent).toBe(selectInput ? "request" : "response");
  });
});
