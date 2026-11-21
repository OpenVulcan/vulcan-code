// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { PublicAiSettingsSnapshot } from "@/types";
import { EconomySettingsArea } from "./economy-settings-area";

// Persisted service values survive unmounts; the component still owns its unsaved draft.
// 服务端保存值跨卸载保留，未保存的草稿仍归组件所有。
const service = vi.hoisted(() => {
  const defaults = { triggerTurns: 40, batchTurns: 20 };
  let saved = defaults;
  const load = async () => ({ economyWindow: saved, defaultEconomyWindow: defaults });
  const save = vi.fn(async (window: PublicAiSettingsSnapshot["economyWindow"]) => {
    saved = window;
    return load();
  });
  return { load, save };
});
vi.mock("@/service/settings-actions-service", () => ({
  loadPublicAiSettings: service.load, saveEconomyWindow: service.save,
}));

// Real numeric inputs exercise validation, explicit saves, remounts and draft-only reset.
// 使用真实数字输入框验证校验、显式保存、重开以及仅修改草稿的重置。
it("validates the pair and only persists an explicit save", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  const container = document.createElement("div");
  const root = createRoot(container);
  document.body.append(container);
  const mount = async (key: string) => act(async () => root.render(
    <I18nProvider initialLocale="en-US"><EconomySettingsArea key={key} /></I18nProvider>,
  ));
  const fields = () => Array.from(container.querySelectorAll("input"));
  const buttons = () => Array.from(container.querySelectorAll("button"));
  const edit = async (input: HTMLInputElement, value: string) => act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  try {
    await mount("first");
    const [trigger, batch] = fields();
    expect(fields().map((input) => input.value)).toEqual(["40", "20"]);
    await edit(trigger, "18");
    expect(container.querySelector('[role="alert"]')).toBeTruthy();
    expect(buttons().at(-1)?.disabled).toBe(true);
    await edit(trigger, "36");
    await edit(batch, "12");
    await act(async () => buttons().at(-1)?.click());
    expect(service.save).toHaveBeenCalledExactlyOnceWith({ triggerTurns: 36, batchTurns: 12 });
    await mount("reopen");
    expect(fields().map((input) => input.value)).toEqual(["36", "12"]);
    await act(async () => buttons()[0].click());
    expect(fields().map((input) => input.value)).toEqual(["40", "20"]);
    await mount("discard");
    expect(fields().map((input) => input.value)).toEqual(["36", "12"]);
    expect(service.save).toHaveBeenCalledTimes(1);
  } finally {
    await act(async () => root.unmount());
    container.remove();
  }
});
