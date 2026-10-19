// @vitest-environment jsdom
import React, { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "./dropdown-menu"

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
