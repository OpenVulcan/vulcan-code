// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import * as settingsService from "@/service/settings-actions-service";
import type { PublicAiCustomProviderModelDefaults, PublicAiSettingsSnapshot } from "@/types";
import { CustomProviderModelDefaultsCard } from "./custom-provider-model-defaults-card";

// Test-owned roots isolate all draft mutations from real provider configuration.
// 测试自有根节点隔离全部草稿变更，不接触真实供应商配置。
const roots: Root[] = [];
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterEach(async () => {
  await act(async () => roots.splice(0).forEach((root) => root.unmount()));
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

// mountDefaults returns the real editor with an explicitly declared synthetic provider layer.
// mountDefaults 返回使用显式声明合成供应商层的真实编辑器。
async function mountDefaults(): Promise<HTMLDivElement> {
  const defaults: PublicAiCustomProviderModelDefaults = { context_limit: 100000, output_limit: 1000,
    input_resources: [], structured_output: "json_object", reasoning: "unsupported", reasoning_control: "fixed_off", reasoning_efforts: ["none"] };
  const container = document.createElement("div");
  const root = createRoot(container);
  roots.push(root);
  document.body.append(container);
  await act(async () => root.render(createElement(I18nProvider, { initialLocale: "zh-CN",
    children: createElement(CustomProviderModelDefaultsCard, { providerId: "custom:test", defaults, customized: true, onSaved: vi.fn() }),
  })));
  return container;
}

// editInput dispatches a native input event so React receives the exact unsanitized operator text.
// editInput 分派原生输入事件，使 React 接收精确且未清洗的操作员文本。
async function editInput(input: HTMLInputElement, value: string): Promise<void> {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("provider default numeric edits", () => {
  it("preserves decimal input and refuses to silently turn 1.5 into 15", async () => {
    const save = vi.spyOn(settingsService, "savePublicAiCustomProviderModelDefaults").mockResolvedValue({} as PublicAiSettingsSnapshot);
    const container = await mountDefaults();
    const input = container.querySelector<HTMLInputElement>("input")!;
    await editInput(input, "1.5");
    expect(input.value).toBe("1.5");
    await act(async () => Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes("保存"))!.click());
    expect(save).not.toHaveBeenCalled();
    expect(container.querySelector("[role=alert]")).not.toBeNull();
  });

  it("serializes repeated saves and keeps the fieldset disabled until rejection settles", async () => {
    let reject!: (reason: Error) => void;
    const save = vi.spyOn(settingsService, "savePublicAiCustomProviderModelDefaults").mockImplementation(() => new Promise((_, rejectRequest) => { reject = rejectRequest; }));
    const container = await mountDefaults();
    await editInput(container.querySelector<HTMLInputElement>("input")!, "200000");
    const button = Array.from(container.querySelectorAll("button")).find((item) => item.textContent?.includes("保存"))!;
    await act(async () => { button.click(); button.click(); });
    expect(save).toHaveBeenCalledTimes(1);
    expect(container.querySelector("fieldset")?.disabled).toBe(true);
    await act(async () => reject(new Error("synthetic failure")));
    expect(container.querySelector("fieldset")?.disabled).toBe(false);
    expect(container.querySelector<HTMLInputElement>("input")?.value).toBe("200000");
    expect(container.textContent).toContain("synthetic failure");
  });
});
