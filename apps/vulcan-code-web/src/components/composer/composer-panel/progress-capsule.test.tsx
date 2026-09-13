// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ComposerSessionProgressCapsule } from "./progress-capsule";
import { I18nProvider } from "@/i18n";
import type { SessionTodolistSummary } from "@/types";

// plan is a detached summary with one completed and one pending task.
// plan 为独立摘要，包含一个已完成和一个待办任务。
const plan: SessionTodolistSummary = { totalCount: 2, completedCount: 1,
  items: [{ step: "Completed task", status: "completed" }, { step: "Pending task", status: "pending" }] };
// host and root own only this test's actual popover and portal lifecycle.
// host 与 root 仅持有此测试中真实浮层及门户的生命周期。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

/** Mounts or updates the actual capsule without replacing its disclosure primitive.
 * 挂载或更新真实胶囊，不替换其展开基础组件。
 * summary supplies the exact task state; resolves after React finishes the update.
 * summary 提供准确任务状态；React 完成更新后结束。
 */
async function render(summary: SessionTodolistSummary) {
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><ComposerSessionProgressCapsule todolist={summary} reviewSummary={null} /></I18nProvider>));
}

it("opens explicitly, hides closed tasks, and dismisses with Escape back to the trigger", async () => {
  await render(plan);
  // trigger is the real keyboard-focusable action; hidden content must not remain in the reading tree.
  // trigger 为真实可键盘聚焦的操作；隐藏内容不得留在阅读树中。
  const trigger = host.querySelector("button")!;
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(document.body.textContent).not.toContain("Pending task");
  await act(async () => trigger.click());
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Pending task");
  await act(async () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  // Radix restores focus after its deferred portal teardown, so wait for that observable result.
  // Radix 在延迟门户卸载之后恢复焦点，因此等待这一可观察结果。
  await vi.waitFor(() => expect(document.activeElement).toBe(trigger));
});

it("keeps the open list synchronized with task updates and permits trigger dismissal", async () => {
  await render(plan);
  const trigger = host.querySelector("button")!;
  await act(async () => trigger.click());
  await render({ ...plan, completedCount: 2, items: plan.items.map((item) => ({ ...item, status: "completed" })) });
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  expect(document.querySelectorAll(".composer-session-progress-row.status-completed")).toHaveLength(2);
  await act(async () => trigger.click());
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
});
