import { describe, expect, it } from "vitest";
import {
  EMPTY_ASSISTANT_DOCK,
  assistantWindowOf,
  closeAssistantWindow,
  detachAssistant,
  mergeAssistantWindows,
  retainAssistants,
  toggleAssistant,
  type AssistantDockState,
} from "./assistant-dock-model";

// layout projects the dock onto window contents and active assistants for compact assertions.
// layout 将停靠状态投影为窗口内容与激活助手，便于简洁断言。
const layout = (state: AssistantDockState) => state.windows.map((window) => ({
  ids: window.assistantIds,
  active: window.activeId,
}));

describe("assistant dock", () => {
  it("joins newly shown assistants to the most recent window and toggles them off again", () => {
    let state = toggleAssistant(EMPTY_ASSISTANT_DOCK, "a");
    state = toggleAssistant(state, "b");
    expect(layout(state)).toEqual([{ ids: ["a", "b"], active: "b" }]);
    state = toggleAssistant(state, "b");
    expect(layout(state)).toEqual([{ ids: ["a"], active: "a" }]);
    state = toggleAssistant(state, "a");
    expect(state.windows).toEqual([]);
  });

  it("detaches an assistant into its own window and merges every window back", () => {
    let state = toggleAssistant(toggleAssistant(EMPTY_ASSISTANT_DOCK, "a"), "b");
    const shared = assistantWindowOf(state, "a")!;
    state = detachAssistant(state, shared.id, "b");
    expect(layout(state)).toEqual([{ ids: ["a"], active: "a" }, { ids: ["b"], active: "b" }]);
    state = toggleAssistant(state, "c");
    expect(assistantWindowOf(state, "c")).toBe(assistantWindowOf(state, "b"));
    state = mergeAssistantWindows(state, shared.id);
    expect(layout(state)).toEqual([{ ids: ["a", "b", "c"], active: "a" }]);
  });

  it("refuses to detach the only assistant of a window", () => {
    const state = toggleAssistant(EMPTY_ASSISTANT_DOCK, "a");
    expect(detachAssistant(state, state.windows[0].id, "a")).toBe(state);
  });

  it("reuses the lowest free window slot so saved geometry stays bounded", () => {
    let state = toggleAssistant(EMPTY_ASSISTANT_DOCK, "a");
    state = detachAssistant(toggleAssistant(state, "b"), state.windows[0].id, "b");
    const first = state.windows[0].id;
    state = toggleAssistant(closeAssistantWindow(state, first), "c");
    state = detachAssistant(toggleAssistant(state, "d"), assistantWindowOf(state, "c")!.id, "d");
    expect(state.windows.map((window) => window.id).sort()).toContain(first);
    expect(new Set(state.windows.map((window) => window.id)).size).toBe(state.windows.length);
  });

  it("drops assistants that were restored or deleted", () => {
    const state = toggleAssistant(toggleAssistant(EMPTY_ASSISTANT_DOCK, "a"), "b");
    expect(layout(retainAssistants(state, new Set(["b"])))).toEqual([{ ids: ["b"], active: "b" }]);
    expect(retainAssistants(state, new Set()).windows).toEqual([]);
  });
});
