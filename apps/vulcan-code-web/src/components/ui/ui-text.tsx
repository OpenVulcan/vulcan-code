import * as React from "react"

// UiText contains the small language surface needed by independently mountable primitives.
// UiText 包含可独立挂载的基础组件所需的精简语言接口。
export interface UiText {
  // Accessible and visible label for the primitive-owned standard close action.
  // 基础组件自有标准关闭动作使用的可访问名称与可见文案。
  readonly close: string
  // Accessible label for a loading indicator without a more specific caller-owned label.
  // 没有调用方专用标签时，加载指示器使用的可访问名称。
  readonly loading: string
  // Accessible name for the application notification viewport.
  // 应用通知视口的可访问名称。
  readonly notifications: string
}

// UiTextContext has an explicit English standalone contract, without browser or document language inference.
// UiTextContext 明确约定独立模式使用英文，不推断浏览器或文档语言。
const UiTextContext = React.createContext<UiText>({ close: "Close", loading: "Loading", notifications: "Notifications" })

// UiTextProvider lets the application localization authority supply current primitive labels.
// UiTextProvider 允许应用本地化权威提供当前基础组件文案。
export const UiTextProvider = UiTextContext.Provider

// useUiText reads application labels when provided and the documented English defaults otherwise.
// useUiText 在应用提供文案时读取应用值，否则使用已明确约定的英文默认值。
// Returns: the current primitive text contract, updated through React context when the application language changes.
// 返回值：当前基础组件文案契约，在应用切换语言时通过 React 上下文更新。
export function useUiText(): UiText {
  return React.useContext(UiTextContext)
}
