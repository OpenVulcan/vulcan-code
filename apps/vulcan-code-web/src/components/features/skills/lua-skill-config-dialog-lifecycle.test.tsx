// @vitest-environment jsdom
import { act } from "react";
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

it("surfaces a storage-domain watcher failure that carries no skill id", async () => {
  const actions = { onDescribeLuaSkillConfig: async () => descriptor,
    onValidateLuaSkillConfig: async () => ({ complete: true, missing: [], invalid: [], businessIssues: [] }),
    onPollLuaSkillConfigEvents: vi.fn().mockResolvedValue({ nextSequence: "9", events: [
      { sequence: "9", type: "config_watch_failed", storeScope: "skills", skillId: null, revision: "3",
        changedKeys: [], source: "watcher", restartRequiredKeys: [], complete: null,
        error: { key: null, code: "CONFIG_WATCH_FAILED", message: "watcher unavailable" } }] })
  } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillConfigDialog actions={actions} skill={skill} open onOpenChange={() => {}} />));
  // The dialog schedules its first poll after 1500 ms, so this waits real time
  // instead of faking timers that the recursive poll would otherwise starve.
  // 弹窗在 1500 毫秒后调度首次轮询，因此这里等待真实时间，而不是伪造会让递归轮询饥饿的定时器。
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 1800)); });
  expect(document.body.textContent).toContain("CONFIG_WATCH_FAILED");
  expect(document.body.textContent).toContain("watcher unavailable");
});

it("ignores storage-domain watcher failures from another store scope", async () => {
  const actions = { onDescribeLuaSkillConfig: async () => descriptor,
    onValidateLuaSkillConfig: async () => ({ complete: true, missing: [], invalid: [], businessIssues: [] }),
    onPollLuaSkillConfigEvents: vi.fn().mockResolvedValue({ nextSequence: "10", events: [
      { sequence: "10", type: "config_watch_failed", storeScope: "system-skills", skillId: null, revision: "4",
        changedKeys: [], source: "watcher", restartRequiredKeys: [], complete: null,
        error: { key: null, code: "CONFIG_WATCH_FAILED", message: "other store watcher unavailable" } }] })
  } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillConfigDialog actions={actions} skill={skill} open onOpenChange={() => {}} />));
  // The dialog waits for the first real poll so the assertion covers the actual retained-event path.
  // 弹窗等待首次真实轮询，使断言覆盖实际的保留事件路径。
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 1800)); });
  expect(document.body.textContent).not.toContain("other store watcher unavailable");
});

it("ignores package events from another store scope even when the skill id matches", async () => {
  const describe = vi.fn().mockResolvedValue(descriptor);
  const actions = { onDescribeLuaSkillConfig: describe,
    onValidateLuaSkillConfig: async () => ({ complete: true, missing: [], invalid: [], businessIssues: [] }),
    onPollLuaSkillConfigEvents: vi.fn().mockResolvedValue({ nextSequence: "10", events: [
      { sequence: "10", type: "config_changed", storeScope: "system-skills", skillId: "fixture", revision: "4",
        changedKeys: ["endpoint"], source: "external_reload", restartRequiredKeys: [], complete: true,
        error: null }] })
  } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillConfigDialog actions={actions} skill={skill} open onOpenChange={() => {}} />));
  // The foreign-store package event must not trigger a descriptor reload for this dialog.
  // 其他存储域的技能包事件不得触发当前弹窗重新加载描述符。
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 1800)); });
  expect(describe).toHaveBeenCalledTimes(1);
});

it("keeps a watcher failure through local writes and clears it after external reload recovery", async () => {
  const poll = vi.fn()
    .mockResolvedValueOnce({ nextSequence: "11", events: [
      { sequence: "11", type: "config_watch_failed", storeScope: "skills", skillId: null, revision: "3",
        changedKeys: [], source: "watcher", restartRequiredKeys: [], complete: null,
        error: { key: null, code: "CONFIG_WATCH_FAILED", message: "watcher needs recovery" } }] })
    .mockResolvedValueOnce({ nextSequence: "12", events: [
      { sequence: "12", type: "config_changed", storeScope: "skills", skillId: "fixture", revision: "3",
        changedKeys: ["endpoint"], source: "local_write", restartRequiredKeys: [], complete: true,
        error: null }] })
    .mockResolvedValue({ nextSequence: "13", events: [
      { sequence: "13", type: "config_changed", storeScope: "skills", skillId: "fixture", revision: "3",
        changedKeys: ["endpoint"], source: "external_reload", restartRequiredKeys: [], complete: true,
        error: null }] });
  const actions = { onDescribeLuaSkillConfig: async () => descriptor,
    onValidateLuaSkillConfig: async () => ({ complete: true, missing: [], invalid: [], businessIssues: [] }),
    onPollLuaSkillConfigEvents: poll } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillConfigDialog actions={actions} skill={skill} open onOpenChange={() => {}} />));
  // The first poll records the watcher failure for this store.
  // 首次轮询记录当前存储域的监听故障。
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 1800)); });
  expect(document.body.textContent).toContain("watcher needs recovery");
  // A local write proves storage access only and must preserve the watcher warning.
  // 本地写入只证明存储可访问，必须保留监听警告。
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 1800)); });
  expect(document.body.textContent).toContain("watcher needs recovery");
  // An external reload from the same store proves watcher recovery and clears the warning.
  // 同一存储域的外部重载证明监听已恢复，并清除警告。
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 1800)); });
  expect(document.body.textContent).not.toContain("watcher needs recovery");
}, 15000);

it("reloads the package store from disk before re-reading the descriptor", async () => {
  // calls records the order of host requests so the descriptor read is proven to follow the disk reload.
  // calls 记录宿主请求顺序，以证明描述读取发生在磁盘重载之后。
  const calls: string[] = [];
  const refresh = vi.fn(async (storeScope: string | null) => {
    calls.push("refresh");
    return [{ storeScope: storeScope ?? "skills", revision: "4", changed: true }];
  });
  const actions = { onDescribeLuaSkillConfig: async () => { calls.push("describe"); return descriptor; },
    onValidateLuaSkillConfig: async () => ({ complete: true, missing: [], invalid: [], businessIssues: [] }),
    onRefreshLuaSkillConfig: refresh,
    onPollLuaSkillConfigEvents: () => new Promise(() => {}) } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillConfigDialog actions={actions} skill={skill} open onOpenChange={() => {}} />));
  calls.length = 0;
  await act(async () => document.querySelector<HTMLButtonElement>('button[aria-label="skills.luaConfig.reload"]')!.click());
  expect(refresh).toHaveBeenCalledWith("skills");
  expect(calls).toEqual(["refresh", "describe"]);
});
