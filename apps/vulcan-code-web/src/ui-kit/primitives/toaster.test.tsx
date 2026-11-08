// @vitest-environment jsdom
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { DEFAULT_DURATION_MS, toast } from "./toast"
import { Toaster } from "./toaster"

// root and container own a real portal-capable React tree with the application's single Toaster.
// root 与 container 持有真实且支持门户的 React 树，其中挂载应用唯一的 Toaster。
let root: Root
let container: HTMLDivElement
let outside: HTMLButtonElement
beforeEach(async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
  vi.useFakeTimers()
  container = document.createElement("div"); document.body.appendChild(container); root = createRoot(container)
  outside = document.createElement("button"); document.body.appendChild(outside)
  await act(async () => root.render(<Toaster />))
})
afterEach(async () => {
  await act(async () => toast.dismiss())
  await act(async () => root.unmount())
  container.remove(); outside.remove()
  vi.useRealTimers()
})

// liveTitles lists the notices currently on screen.
// liveTitles 列出当前显示的提示。
function liveTitles(): string[] {
  return [...document.querySelectorAll('[data-slot="toast"] .vk-toast-title')].map((title) => title.textContent ?? "")
}

// elapse advances the lifetime timers past one default lifetime.
// elapse 将存续计时器推进超过一个默认存续时间。
async function elapse(): Promise<void> {
  await act(async () => { vi.advanceTimersByTime(DEFAULT_DURATION_MS + 1) })
}

it("keeps later notices expiring after a notice is closed with its own focused close button", async () => {
  await act(async () => { toast.success("first") })
  // Pressing the close button focuses it first; the press then removes the focused button with its notice.
  // 按下关闭按钮会先聚焦它；随后按下动作将获得焦点的按钮连同提示一起移除。
  const close = document.querySelector<HTMLButtonElement>('[data-slot="toast"] .vk-toast-close')!
  await act(async () => { close.focus() })
  await act(async () => { close.click() })
  expect(liveTitles()).toEqual([])
  await act(async () => { toast.success("second") })
  await elapse()
  expect(liveTitles()).toEqual([])
})

it("keeps later notices expiring after a notice is closed through its action button", async () => {
  await act(async () => { toast.success("first", { action: { label: "Open", onClick: () => undefined } }) })
  const action = document.querySelector<HTMLButtonElement>('[data-slot="toast"] .vk-toast-action')!
  await act(async () => { action.focus() })
  await act(async () => { action.click() })
  await act(async () => { toast.info("second") })
  await elapse()
  expect(liveTitles()).toEqual([])
})

it("pauses while focus is inside the stack and resumes once focus leaves", async () => {
  await act(async () => { toast.success("read me") })
  await act(async () => { document.querySelector<HTMLButtonElement>('[data-slot="toast"] .vk-toast-close')!.focus() })
  await elapse()
  expect(liveTitles()).toEqual(["read me"])
  await act(async () => { outside.focus() })
  await elapse()
  expect(liveTitles()).toEqual([])
})
