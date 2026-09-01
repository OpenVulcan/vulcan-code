// @vitest-environment jsdom
// 使用 jsdom 验证模型扩展输入卡与返回卡的独立展开行为。
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
  // Input starts folded, output starts expanded, and both update only their own expansion identity.
  // 输入默认折叠、返回默认展开，并且二者仅更新各自的展开身份。
  it("uses independent input-folded and output-expanded defaults", async () => {
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
    const inputButton = host.querySelector<HTMLButtonElement>('[data-model-extension-payload="input"] button');
    // outputButton is the exact control owned by the output payload card.
    // outputButton 是返回载荷卡持有的精确控件。
    const outputButton = host.querySelector<HTMLButtonElement>('[data-model-extension-payload="output"] button');
    expect(inputButton?.getAttribute("aria-expanded")).toBe("false");
    expect(outputButton?.getAttribute("aria-expanded")).toBe("true");
    expect(inputButton?.querySelector(".lucide-log-in")).not.toBeNull();
    expect(outputButton?.querySelector(".lucide-log-out")).not.toBeNull();

    await act(async () => inputButton?.click());
    expect(inputButton?.getAttribute("aria-expanded")).toBe("true");
    expect(outputButton?.getAttribute("aria-expanded")).toBe("true");

    await act(async () => outputButton?.click());
    expect(inputButton?.getAttribute("aria-expanded")).toBe("true");
    expect(outputButton?.getAttribute("aria-expanded")).toBe("false");
  });
});
