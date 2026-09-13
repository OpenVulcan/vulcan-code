// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { LuaSkillConfigDescriptor, LuaSkillConfigItem, LuaSkillSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";
import { LuaSkillConfigDialog } from "./lua-skill-config-dialog";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: (key: string) => key }) }));
// item builds a complete source-declared string field for the isolated configuration form.
// item 为隔离配置表单构建完整的源码声明字符串字段。
// key and sensitive select the declaration; returns a non-secret host-safe value.
// key 与 sensitive 选择声明；返回不含秘密的宿主安全值。
function item(key: string, sensitive = false): LuaSkillConfigItem {
  return { key, sensitive, type: "string", required: false, description: "", constraints: {}, options: [],
    advanced: false, restartRequired: false, deprecated: false, state: "configured", satisfied: true,
    value: sensitive ? null : "old-value" };
}
// descriptor and skill contain the exact projected fields consumed by this dialog.
// descriptor 与 skill 包含此弹窗读取的精确投影字段。
const descriptor: LuaSkillConfigDescriptor = { skillId: "fixture", skillVersion: "1", complete: true,
  revision: "3", storeScope: "skills", missingCount: 0, invalidCount: 0, businessIssueCount: 0,
  orphanedCount: 0, orphaned: [], items: [item("api_token", true), item("endpoint")] };
const skill = { skillId: "fixture", displayName: "Fixture" } as LuaSkillSummary;
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// edit changes the real controlled browser input through a native input event.
// edit 通过原生输入事件修改真实受控浏览器输入框。
// key chooses an exact declared field, value is its draft; returns after React commits it.
// key 选择精确声明字段，value 是草稿；在 React 提交后返回。
async function edit(key: string, value: string) {
  const input = document.getElementById(`lua-skill-config-${key}`) as HTMLInputElement;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

it("clears committed sensitive drafts even when the follow-up validation fails", async () => {
  const validate = vi.fn().mockResolvedValueOnce({ complete: true, missing: [], invalid: [], businessIssues: [] })
    .mockRejectedValue(new Error("validation unavailable"));
  const save = vi.fn().mockResolvedValue({ descriptor: { ...descriptor, revision: "4" } });
  const actions = { onDescribeLuaSkillConfig: async () => descriptor, onValidateLuaSkillConfig: validate,
    onSetLuaSkillConfig: save, onPollLuaSkillConfigEvents: () => new Promise(() => {}) } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillConfigDialog actions={actions} skill={skill} open onOpenChange={() => {}} />));
  await edit("api_token", "synthetic-secret");
  await act(async () => [...document.querySelectorAll("button")].find((button) => button.textContent === "skills.luaConfig.save")!.click());
  expect(save).toHaveBeenCalledTimes(1);
  expect((document.getElementById("lua-skill-config-api_token") as HTMLInputElement).value).toBe("");
  expect(document.body.textContent).toContain("validation unavailable");
  expect(document.body.textContent).toContain("rev 4");
});

it("deleting one key preserves the unsaved draft of another key", async () => {
  const deleted = { ...descriptor, revision: "4", items: [item("api_token", true), { ...item("endpoint"), state: "unset", value: null }] };
  const actions = { onDescribeLuaSkillConfig: async () => descriptor,
    onValidateLuaSkillConfig: async () => ({ complete: true, missing: [], invalid: [], businessIssues: [] }),
    onDeleteLuaSkillConfig: vi.fn().mockResolvedValue({ descriptor: deleted }),
    onPollLuaSkillConfigEvents: () => new Promise(() => {}) } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillConfigDialog actions={actions} skill={skill} open onOpenChange={() => {}} />));
  await edit("api_token", "uncommitted-secret");
  const row = document.getElementById("lua-skill-config-endpoint")!.closest(".lua-skill-config-field")!;
  await act(async () => row.querySelector<HTMLButtonElement>('button[aria-label="skills.luaConfig.deleteValue"]')!.click());
  expect((document.getElementById("lua-skill-config-api_token") as HTMLInputElement).value).toBe("uncommitted-secret");
  expect((document.getElementById("lua-skill-config-endpoint") as HTMLInputElement).value).toBe("");
});
