// @vitest-environment jsdom
import React, { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "./dropdown-menu"

// container and root own one real mounted menu tree per test.
// container 与 root 为每项测试持有一个真实挂载的菜单树。
let container: HTMLDivElement
let root: Root
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} })
  container = document.createElement("div")
  document.body.appendChild(container)
  root = createRoot(container)
})
afterEach(async () => {
  await act(async () => root.unmount())
  container.remove()
  vi.unstubAllGlobals()
})

// mountMenu renders one closed menu next to an outside focus target.
// mountMenu 渲染一个关闭的菜单及一个外部焦点目标。
// Returns: the trigger and the outside button.
// 返回值：触发器与外部按钮。
async function mountMenu() {
  await act(async () => root.render(<>
    <DropdownMenu>
      <DropdownMenuTrigger>Actions</DropdownMenuTrigger>
      <DropdownMenuContent><DropdownMenuItem>Rename</DropdownMenuItem></DropdownMenuContent>
    </DropdownMenu>
    <button type="button" data-outside>Outside</button>
  </>))
  return {
    trigger: container.querySelector<HTMLButtonElement>('[data-slot="dropdown-menu-trigger"]')!,
    outside: container.querySelector<HTMLButtonElement>("[data-outside]")!,
  }
}

// openWithKeyboard opens the menu the way a keyboard user does.
// openWithKeyboard 以键盘用户的方式打开菜单。
async function openWithKeyboard(trigger: HTMLElement) {
  await act(async () => {
    trigger.focus()
    trigger.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "ArrowDown" }))
  })
  await vi.waitFor(() => expect(document.querySelector('[role="menu"]')).not.toBeNull())
}

describe("shared dropdown menu focus restoration", () => {
  // A keyboard close returns focus to the trigger so the user keeps their place.
  // 键盘关闭将焦点归还触发器，使用户保持原位置。
  it("returns focus to the trigger after Escape", async () => {
    const { trigger } = await mountMenu()
    await openWithKeyboard(trigger)
    await act(async () => {
      document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" }))
    })
    await vi.waitFor(() => expect(document.querySelector('[role="menu"]')).toBeNull())
    expect(document.activeElement).toBe(trigger)
  })

  // An outside press means the user moved on, so focus is not pulled back to the trigger.
  // 外部按下表示用户已转向别处，因此不把焦点拉回触发器。
  it("leaves focus alone after an outside pointer dismissal", async () => {
    const { trigger, outside } = await mountMenu()
    await openWithKeyboard(trigger)
    await act(async () => {
      outside.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }))
      outside.focus()
    })
    await vi.waitFor(() => expect(document.querySelector('[role="menu"]')).toBeNull())
    expect(document.activeElement).toBe(outside)
  })
})

describe("shared dropdown submenus", () => {
  // openSubmenus lists the labels of the submenu surfaces currently shown.
  // openSubmenus 列出当前显示的子菜单表面的标签。
  const openSubmenus = () => Array.from(document.querySelectorAll('[data-slot="dropdown-menu-sub-content"]')).map((node) => node.textContent)
  // hover moves the pointer over one row the way a mouse does.
  // hover 以鼠标的方式将指针移到一行上。
  const hover = async (label: string) => {
    const row = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitem"]')).find((node) => node.textContent === label)!
    await act(async () => { row.dispatchEvent(new MouseEvent("pointermove", { bubbles: true })) })
  }

  // Hovering several submenu rows and then a plain row never leaves more than one submenu showing.
  // 依次悬停多个子菜单行再悬停普通行，始终不会同时显示多个子菜单。
  it("keeps at most one submenu open while the pointer moves between rows", async () => {
    vi.useFakeTimers()
    try {
      await act(async () => root.render(
        <DropdownMenu open>
          <DropdownMenuTrigger>App</DropdownMenuTrigger>
          <DropdownMenuContent>
            {["Theme", "Text", "Width"].map((name) => (
              <DropdownMenuSub key={name}>
                <DropdownMenuSubTrigger>{name}</DropdownMenuSubTrigger>
                <DropdownMenuSubContent><DropdownMenuItem>{name} choice</DropdownMenuItem></DropdownMenuSubContent>
              </DropdownMenuSub>
            ))}
            <DropdownMenuItem>Reduce motion</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>,
      ))
      await hover("Theme")
      expect(openSubmenus()).toEqual(["Theme choice"])
      for (const name of ["Text", "Width"]) {
        await hover(name)
        await act(async () => { vi.advanceTimersByTime(400) })
        expect(openSubmenus()).toEqual([`${name} choice`])
      }
      await hover("Reduce motion")
      await act(async () => { vi.advanceTimersByTime(400) })
      expect(openSubmenus()).toEqual([])
    } finally {
      vi.useRealTimers()
    }
  })
})
