// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput, InputGroupTextarea } from "./input-group";
import { CommandDialog } from "./command";

// container and root own one real primitive tree for each behavior regression.
// container 与 root 为每项行为回归持有一个真实基础组件树。
let container: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals(); });

// Addon clicks must use the declared editable control without stealing action-button focus.
// 附加项点击必须使用声明的可编辑控件，不抢夺操作按钮焦点。
it.each(["input", "textarea"] as const)("focuses the %s from an addon but preserves its action button", async kind => {
  const action = vi.fn();
  await act(async () => root.render(<InputGroup>{kind === "input" ? <InputGroupInput aria-label="edit" /> : <InputGroupTextarea aria-label="edit" />}<InputGroupAddon><span data-label>Focus editor</span><InputGroupButton onClick={action}>Action</InputGroupButton></InputGroupAddon></InputGroup>));
  await act(async () => container.querySelector<HTMLElement>("[data-label]")!.click());
  expect(document.activeElement).toBe(container.querySelector(kind));
  const button = container.querySelector<HTMLButtonElement>("button")!;
  await act(async () => { button.focus(); button.click(); });
  expect(action).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(button);
});

// An unused-but-exported dialog must not leave orphaned headings when closed.
// 尚无业务调用但已导出的弹窗关闭时不能遗留孤立标题。
it("owns accessible command labels only while its dialog is mounted", async () => {
  await act(async () => root.render(<CommandDialog open={false} title="Commands" description="Local only"><button>Run</button></CommandDialog>));
  expect(document.querySelector('[data-slot="dialog-title"]')).toBeNull();
  await act(async () => root.render(<CommandDialog open title="Commands" description="Local only"><button>Run</button></CommandDialog>));
  const dialog = document.querySelector('[role="dialog"]')!;
  expect(dialog.contains(document.getElementById(dialog.getAttribute("aria-labelledby")!))).toBe(true);
  expect(dialog.contains(document.getElementById(dialog.getAttribute("aria-describedby")!))).toBe(true);
});
