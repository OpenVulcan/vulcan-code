// @vitest-environment jsdom
import * as React from "react"
import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterEach, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { Sidebar, SidebarProvider, SidebarTrigger, useSidebarNavigation } from "./sidebar"

// createNavigationFixture mounts actual drawer primitives with a controlled desktop preference.
// createNavigationFixture 使用受控桌面偏好挂载真实抽屉组件。
// Parameters: width is the initial viewport width; collapsible selects the desktop collapse mode. Returns DOM handles and action spies.
// 参数：width 为初始视口宽度；collapsible 选择桌面折叠模式。返回 DOM 句柄及动作记录器。
async function createNavigationFixture(width: number, collapsible = true) {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true)
  window.innerWidth = width
  // changed and selected track persistence and exact destination identity independently.
  // changed 和 selected 分别跟踪持久化调用与准确的导航目标身份。
  const changed = vi.fn()
  const selected = vi.fn()
  // Destination uses the production navigation wrapper and returns one actionable entry.
  // Destination 使用生产导航包装器，返回一个可操作入口。
  function Destination() {
    // navigate preserves the destination payload while closing the mobile drawer.
    // navigate 保留导航目标参数，并关闭手机抽屉。
    const navigate = useSidebarNavigation()
    return <button data-testid="destination" onClick={() => navigate(selected)("project-stable-id")}>项目</button>
  }
  // host and root own the mounted UI lifetime.
  // host 和 root 管理挂载界面的生命周期。
  const host = document.createElement("div")
  document.body.append(host)
  const root = createRoot(host)
  await act(async () => root.render(
    <I18nProvider initialLocale="zh-CN">
      <SidebarProvider collapsible={collapsible} open onOpenChange={changed}>
        <Sidebar><Destination /></Sidebar>
        <SidebarTrigger />
      </SidebarProvider>
    </I18nProvider>,
  ))
  return { host, root, changed, selected }
}

afterEach(() => vi.unstubAllGlobals())

it("opens phone navigation, selects the exact destination, and preserves the desktop preference", async () => {
  // view contains the real portaled drawer and controlled preference observer.
  // view 包含真实传送抽屉及受控偏好观察器。
  const view = await createNavigationFixture(390)
  try {
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    await act(async () => view.host.querySelector<HTMLButtonElement>('[aria-label="切换侧栏"]')!.click())
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()
    await act(async () => document.querySelector<HTMLButtonElement>('[data-testid="destination"]')!.click())
    expect(view.selected).toHaveBeenCalledExactlyOnceWith("project-stable-id")
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(view.changed).not.toHaveBeenCalled()
    await vi.waitFor(() => expect(document.activeElement).toBe(view.host.querySelector('[aria-label="切换侧栏"]')))
    await act(async () => { window.innerWidth = 1200; window.dispatchEvent(new Event("resize")) })
    expect(view.host.querySelector('[data-slot="sidebar"]')?.getAttribute("data-state")).toBe("expanded")
    expect(view.changed).not.toHaveBeenCalled()
  } finally { await act(async () => view.root.unmount()); view.host.remove() }
})

it("dismisses phone navigation with Escape and keeps desktop toggling controlled", async () => {
  // view exercises keyboard dismissal before returning to the desktop shell.
  // view 在返回桌面外壳之前验证键盘关闭行为。
  const view = await createNavigationFixture(360)
  try {
    await act(async () => view.host.querySelector<HTMLButtonElement>('[aria-label="切换侧栏"]')!.click())
    await act(async () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })))
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(view.changed).not.toHaveBeenCalled()
    await act(async () => { window.innerWidth = 1200; window.dispatchEvent(new Event("resize")) })
    await act(async () => view.host.querySelector<HTMLButtonElement>('[aria-label="切换侧栏"]')!.click())
    expect(view.changed).toHaveBeenCalledExactlyOnceWith(false)
  } finally { await act(async () => view.root.unmount()); view.host.remove() }
})

it("keeps a non-collapsible panel open on desktop while the phone drawer still opens", async () => {
  // view mounts a panel that, like feature menus and settings, never collapses on desktop.
  // view 挂载一个与功能菜单、设置相同、在桌面上永不折叠的面板。
  const view = await createNavigationFixture(1200, false)
  try {
    const trigger = () => view.host.querySelector<HTMLButtonElement>('[aria-label="切换侧栏"]')!
    expect(view.host.querySelector('[data-slot="sidebar-wrapper"]')?.getAttribute("data-panel-collapsible")).toBe("false")
    await act(async () => trigger().click())
    expect(view.changed).not.toHaveBeenCalled()
    expect(view.host.querySelector('[data-slot="sidebar"]')?.getAttribute("data-state")).toBe("expanded")
    // Phones keep the drawer, because it is the only way to reach the panel there.
    // 手机保留抽屉，因为那是到达面板的唯一途径。
    await act(async () => { window.innerWidth = 390; window.dispatchEvent(new Event("resize")) })
    await act(async () => trigger().click())
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()
    expect(view.changed).not.toHaveBeenCalled()
  } finally { await act(async () => view.root.unmount()); view.host.remove() }
})
