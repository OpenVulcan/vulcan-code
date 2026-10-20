import * as React from "react"

// Keep context identity independent of component refreshes and their UI dependencies.
// 将上下文身份与组件热更新及其界面依赖隔离。
// SidebarContextValue stores the collapsible sidebar state for one inset shell.
// SidebarContextValue 保存单个 inset 外壳内的可折叠侧栏状态。
export interface SidebarContextValue {
  // Whether navigation uses a modal drawer instead of desktop width.
  // 导航是否使用模态抽屉而非占用桌面宽度。
  isMobile: boolean
  // Dismisses mobile navigation without changing the desktop preference.
  // 关闭手机导航且不修改桌面的展开偏好。
  closeMobileSidebar: () => void
  // Restores keyboard focus to the navigation trigger in the owning shell.
  // 将键盘焦点恢复到所属外壳的导航触发器。
  focusTrigger: () => void
  // Current visual state used by sidebar data attributes.
  // 当前用于侧栏 data 属性的视觉状态。
  state: "expanded" | "collapsed"
  // Whether the sidebar is currently open.
  // 当前侧栏是否处于展开状态。
  open: boolean
  // Updates the open state.
  // 更新展开状态。
  setOpen: (open: boolean) => void
  // Toggles the sidebar open state.
  // 切换侧栏展开状态。
  toggleSidebar: () => void
}

// SidebarContext shares state between Sidebar, SidebarInset, and SidebarTrigger.
// SidebarContext 在 Sidebar、SidebarInset 与 SidebarTrigger 之间共享状态。
export const SidebarContext = React.createContext<SidebarContextValue | null>(null)

// useSidebar returns the nearest sidebar context.
// useSidebar 返回最近的侧栏上下文。
// Parameters: none. Returns the owning provider state; throws when no provider exists.
// 参数：无。返回所属提供者状态；缺少提供者时抛出异常。
export function useSidebar(): SidebarContextValue {
  // context must come from the same stable module as the provider's context object.
  // context 必须来自与提供者上下文对象相同的稳定模块。
  const context = React.useContext(SidebarContext)
  if (!context) {
    throw new Error("useSidebar must be used within a SidebarProvider.")
  }
  return context
}

// useSidebarNavigation wraps a destination callback and dismisses only the mobile drawer.
// useSidebarNavigation 包装导航目标回调，仅关闭手机抽屉。
// Parameters: none. Returns a wrapper preserving the callback arguments and result.
// 参数：无。返回保留原回调参数和结果的包装函数。
export function useSidebarNavigation() {
  const { closeMobileSidebar } = useSidebar()
  return React.useCallback(<Args extends unknown[], Result,>(action: (...args: Args) => Result) => (...args: Args): Result => {
    closeMobileSidebar()
    return action(...args)
  }, [closeMobileSidebar])
}

