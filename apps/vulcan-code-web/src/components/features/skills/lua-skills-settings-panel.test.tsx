// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import type { SkillActionProps } from "./skill-area-types";
import { LuaSkillsSettingsPanel } from "./lua-skills-settings-panel";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: (key: string) => key }) }));
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// editNumber changes the first declared VM pool field through its actual controlled input.
// editNumber 通过真实受控输入框修改首个已声明 VM 池字段。
// value is the new integer draft; returns after React commits the edit.
// value 是新整数草稿；在 React 提交编辑后返回。
async function editNumber(value: string) {
  const input = host.querySelector<HTMLInputElement>('input[type="number"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

it("retains local settings across external updates until the user explicitly reloads", async () => {
  const catalog = browserFallbackSnapshot().luaSkills;
  const actions = {} as SkillActionProps;
  await act(async () => root.render(<LuaSkillsSettingsPanel catalog={catalog} actions={actions} />));
  const original = host.querySelector<HTMLInputElement>('input[type="number"]')!.value;
  const changed = String(Number(original) + 1);
  await editNumber(changed);
  const updated = { ...catalog, runtimeSettings: { ...catalog.runtimeSettings,
    aiTools: { ...catalog.runtimeSettings.aiTools, enableRunLua: !catalog.runtimeSettings.aiTools.enableRunLua } } };
  await act(async () => root.render(<LuaSkillsSettingsPanel catalog={updated} actions={actions} />));
  expect(host.querySelector<HTMLInputElement>('input[type="number"]')!.value).toBe(changed);
  expect(host.textContent).toContain("Settings changed elsewhere");
  await act(async () => [...host.querySelectorAll("button")].find((button) => button.textContent === "Discard draft and load latest settings")!.click());
  expect(host.querySelector<HTMLInputElement>('input[type="number"]')!.value).toBe(original);
  expect(host.textContent).not.toContain("Settings changed elsewhere");
});

it("allows a failed save to retry while serializing writes and disabling the form", async () => {
  const catalog = browserFallbackSnapshot().luaSkills;
  let complete!: () => void;
  const save = vi.fn().mockRejectedValueOnce(new Error("storage unavailable"))
    .mockImplementation(() => new Promise<void>((resolve) => { complete = resolve; }));
  await act(async () => root.render(<LuaSkillsSettingsPanel catalog={catalog} actions={{ onSaveLuaSkillsRuntimeSettings: save } as unknown as SkillActionProps} />));
  await editNumber(String(Number(host.querySelector<HTMLInputElement>('input[type="number"]')!.value) + 1));
  const button = [...host.querySelectorAll("button")].find((value) => value.textContent === "skills.luaSettings.save")!;
  await act(async () => button.click());
  expect(host.textContent).toContain("storage unavailable");
  expect(button.disabled).toBe(false);
  await act(async () => { button.click(); button.click(); });
  expect(save).toHaveBeenCalledTimes(2);
  expect(host.querySelector("fieldset")!.disabled).toBe(true);
  await act(async () => complete());
  expect(host.querySelector("fieldset")!.disabled).toBe(false);
});
