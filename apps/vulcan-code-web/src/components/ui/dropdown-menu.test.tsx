import React from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

// TestDismissEvent models the cancellable Radix focus-management event used by the shared menu.
// TestDismissEvent 模拟共享菜单使用的可取消 Radix 焦点管理事件。
interface TestDismissEvent {
  // Whether one handler has cancelled the default Radix behavior.
  // 是否已有处理器取消 Radix 默认行为。
  defaultPrevented: boolean
  // Cancels the default Radix behavior.
  // 取消 Radix 默认行为。
  preventDefault: () => void
}

// CapturedContentProps contains only the interaction callbacks required by the regression test.
// CapturedContentProps 仅包含回归测试所需的交互回调。
interface CapturedContentProps {
  // Runs when one pointer interaction occurs outside the menu.
  // 指针在菜单外部交互时运行。
  onInteractOutside?: (event: TestDismissEvent) => void
  // Runs before Radix restores focus after menu closure.
  // Radix 在菜单关闭后恢复焦点前运行。
  onCloseAutoFocus?: (event: TestDismissEvent) => void
}

// mockState exposes the props passed into the mocked Radix content primitive.
// mockState 暴露传入模拟 Radix 内容原语的属性。
const mockState = vi.hoisted(() => ({
  contentProps: null as CapturedContentProps | null,
}))

vi.mock("radix-ui", async () => {
  // reactModule creates inert mock elements without depending on a browser DOM.
  // reactModule 在不依赖浏览器 DOM 的情况下创建静态模拟元素。
  const reactModule = await import("react")
  return {
    DropdownMenu: {
      // Portal keeps children inline for server-rendered callback inspection.
      // Portal 为服务端渲染回调检查将子元素保留在原位。
      Portal: ({ children }: React.PropsWithChildren) => children,
      // Content captures the focus callbacks installed by the shared wrapper.
      // Content 捕获共享包装器安装的焦点回调。
      Content: (props: CapturedContentProps) => {
        mockState.contentProps = props
        return reactModule.createElement("div")
      },
    },
  }
})

import { DropdownMenuContent } from "./dropdown-menu"

// createDismissEvent builds one mutable cancellable event for direct callback verification.
// createDismissEvent 构建一个可变的可取消事件，用于直接验证回调。
//
// Returns an event whose defaultPrevented flag records preventDefault calls.
// 返回一个通过 defaultPrevented 标记记录 preventDefault 调用的事件。
function createDismissEvent(): TestDismissEvent {
  // event records whether the tested callback cancelled default focus management.
  // event 记录被测回调是否取消了默认焦点管理。
  const event: TestDismissEvent = {
    defaultPrevented: false,
    preventDefault: () => {
      event.defaultPrevented = true
    },
  }
  return event
}

describe("shared dropdown menu focus restoration", () => {
  // Verifies pointer dismissal does not lock focus on the trigger while a later keyboard close still restores it.
  // 验证指针关闭不会把焦点锁在触发按钮上，同时后续键盘关闭仍会恢复焦点。
  it("filters trigger focus restoration only after outside interaction", () => {
    renderToStaticMarkup(<DropdownMenuContent>menu</DropdownMenuContent>)
    // contentProps are captured synchronously by the mocked Radix content primitive.
    // contentProps 由模拟 Radix 内容原语同步捕获。
    const contentProps = mockState.contentProps
    expect(contentProps?.onInteractOutside).toBeTypeOf("function")
    expect(contentProps?.onCloseAutoFocus).toBeTypeOf("function")

    const outsideEvent = createDismissEvent()
    contentProps?.onInteractOutside?.(outsideEvent)
    const pointerCloseEvent = createDismissEvent()
    contentProps?.onCloseAutoFocus?.(pointerCloseEvent)
    expect(pointerCloseEvent.defaultPrevented).toBe(true)

    const keyboardCloseEvent = createDismissEvent()
    contentProps?.onCloseAutoFocus?.(keyboardCloseEvent)
    expect(keyboardCloseEvent.defaultPrevented).toBe(false)
  })
})
