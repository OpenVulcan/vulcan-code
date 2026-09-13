// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "./tooltip";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger } from "./dropdown-menu";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSub, ContextMenuSubContent, ContextMenuSubTrigger, ContextMenuTrigger } from "./context-menu";

// container and root own one real mounted floating-control tree per test.
// container 与 root 为每项测试持有一个真实挂载的浮层控件树。
let container: HTMLDivElement;
let root: Root;
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  container = document.createElement("div"); document.body.appendChild(container); root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals(); });

// mountTooltip exposes real tooltip focus state plus an external keyboard-navigation destination.
// mountTooltip 暴露真实提示焦点状态及外部键盘导航目标。
// Returns: exact mounted trigger and outside button used to simulate browser focus transitions.
// 返回值：用于模拟浏览器焦点转换的准确已挂载触发器和外部按钮。
async function mountTooltip() {
  await act(async () => root.render(<TooltipProvider delayDuration={0}>
    <Tooltip><TooltipTrigger>Help</TooltipTrigger><TooltipContent>Accessible help</TooltipContent></Tooltip>
    <button type="button" data-outside>Outside</button>
  </TooltipProvider>));
  return { trigger: container.querySelector<HTMLButtonElement>('[data-slot="tooltip-trigger"]')!,
    outside: container.querySelector<HTMLButtonElement>("[data-outside]")! };
}

describe("shared floating control interactions", () => {
  // Real context submenus must escape transformed ancestors and retain keyboard access to the final entry.
  // 真实右键子菜单必须脱离带变换的祖先，并保留最后一项的键盘可达性。
  it("portals an opened context submenu and navigates to its final item", async () => {
    await act(async () => root.render(<ContextMenu><ContextMenuTrigger>Files</ContextMenuTrigger><ContextMenuContent><ContextMenuSub open><ContextMenuSubTrigger>Folders</ContextMenuSubTrigger><ContextMenuSubContent>{Array.from({ length: 60 }, (_, index) => <ContextMenuItem key={index}>Folder {index + 1}</ContextMenuItem>)}</ContextMenuSubContent></ContextMenuSub></ContextMenuContent></ContextMenu>));
    await act(async () => container.querySelector<HTMLElement>('[data-slot="context-menu-trigger"]')!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 100, clientY: 100 })));
    // submenu resolves the actual portal produced by the context menu wrapper.
    // submenu 解析右键菜单包装器产出的实际门户。
    const submenu = document.querySelector<HTMLElement>('[data-slot="context-menu-sub-content"]')!;
    expect(submenu.parentElement!.closest('[role="menu"]')).toBeNull();
    const entries = submenu.querySelectorAll<HTMLElement>('[role="menuitem"]');
    expect(entries).toHaveLength(60);
    await act(async () => { entries[0].focus(); entries[0].dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "End" })); });
    await vi.waitFor(() => expect(document.activeElement).toBe(entries[59]));
  });
  it.each(["Tab", "ArrowRight"])("restores keyboard tooltip access with %s after a pointer activation", async (key) => {
    const { trigger, outside } = await mountTooltip();
    await act(async () => {
      trigger.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })); trigger.focus();
      document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true })); trigger.click(); outside.focus();
    });
    expect(trigger.getAttribute("data-state")).toBe("closed");
    await act(async () => {
      outside.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key })); trigger.focus();
    });
    expect(trigger.getAttribute("data-state")).toBe("instant-open");
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("Accessible help");
  });

  it("keeps pointer-origin restoration quiet on Escape without permanently blocking later keyboard focus", async () => {
    const { trigger, outside } = await mountTooltip();
    await act(async () => {
      trigger.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })); trigger.focus();
      document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true })); outside.focus();
      outside.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" })); trigger.focus();
    });
    expect(trigger.getAttribute("data-state")).toBe("closed");
    await act(async () => {
      outside.focus(); outside.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Tab", shiftKey: true })); trigger.focus();
    });
    expect(trigger.getAttribute("data-state")).toBe("instant-open");
  });

  it("bounds a long real submenu and retains End-key navigation to the final item", async () => {
    await act(async () => root.render(<DropdownMenu open><DropdownMenuTrigger>Attach</DropdownMenuTrigger>
      <DropdownMenuContent><DropdownMenuSub open><DropdownMenuSubTrigger>Libraries</DropdownMenuSubTrigger>
        <DropdownMenuSubContent>{Array.from({ length: 80 }, (_, index) => <DropdownMenuItem key={index}>Library {index + 1}</DropdownMenuItem>)}</DropdownMenuSubContent>
      </DropdownMenuSub></DropdownMenuContent></DropdownMenu>));
    const submenu = document.querySelector<HTMLElement>('[data-slot="dropdown-menu-sub-content"]')!;
    // A transformed ancestor menu must not become the fixed-position containing block.
    // 带变换的祖先菜单不能成为固定定位的包含块。
    expect(submenu.parentElement?.closest('[role="menu"]')).toBeNull();
    const items = submenu.querySelectorAll<HTMLElement>('[role="menuitem"]');
    expect(items).toHaveLength(80);
    expect(submenu.className).toContain("max-h-(--radix-dropdown-menu-content-available-height)");
    expect(submenu.className).toContain("overflow-y-auto");
    await act(async () => { items[0].focus(); items[0].dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "End" })); });
    await vi.waitFor(() => expect(document.activeElement).toBe(items[79]));
  });
});
