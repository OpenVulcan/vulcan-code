// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { DEFAULT_APPEARANCE, setAppearance } from "@/ui-kit/appearance";
import { AppearanceSettingsArea } from "./appearance-settings-area";

vi.mock("@/host-shell", () => ({ createShellClient: () => ({}) }));

// host and root own one settings render per test; preferences are reset afterwards.
// host 与 root 在每个测试中持有一次设置渲染；测试后重置偏好。
let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  for (const [key, value] of Object.entries(DEFAULT_APPEARANCE)) setAppearance(key as keyof typeof DEFAULT_APPEARANCE, value as never);
});

// Each ambience option writes its value onto the document root, where the kit stylesheets read it.
// 每个氛围选项都把取值写到文档根节点，由 kit 样式读取。
it("applies every ambience choice to the document root", async () => {
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><AppearanceSettingsArea /></I18nProvider>));
  const group = host.querySelector<HTMLElement>('[role="radiogroup"][aria-label="Ambience"]');
  expect(group, "ambience segmented control").not.toBeNull();
  const options = [...group!.querySelectorAll<HTMLButtonElement>('[role="radio"]')];
  expect(options).toHaveLength(3);
  for (const [index, value] of (["off", "calm", "live"] as const).entries()) {
    await act(async () => options[index].click());
    expect(document.documentElement.dataset.vkAmbience).toBe(value);
    expect(options[index].getAttribute("aria-checked")).toBe("true");
  }
});
