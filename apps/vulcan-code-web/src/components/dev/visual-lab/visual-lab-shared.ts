import type { SyntheticEvent } from "react"

// VisualLabText picks the Chinese or English copy for the active locale.
// VisualLabText 按当前语言选择中文或英文文案。
export type VisualLabText = (zh: string, en: string) => string

// VisualLabDemoProps is the contract every visual lab category receives from the shell.
// VisualLabDemoProps 是每个视觉实验室分类从外壳接收的契约。
export interface VisualLabDemoProps {
  // text binds the active locale to the compact copy selector.
  // text 将当前语言绑定到紧凑文案选择器。
  text: VisualLabText
  // onAction records one local demo activation in the shell status line; it never calls product services.
  // onAction 在外壳状态行记录一次本地演示操作；从不调用产品服务。
  onAction: (label: string) => void
}

// visualLabText selects compact lab copy for the active locale.
// visualLabText 根据当前语言选择紧凑的实验室文案。
// Parameters: locale is the active locale; zh and en are the two copies. Returns the matching copy.
// 参数：locale 为当前语言；zh 与 en 为两种文案。返回匹配的文案。
export function visualLabText(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}

// actionLabel reads the accessible label of the element that triggered a demo event.
// actionLabel 读取触发演示事件的元素的无障碍标签。
// Parameters: event is any React event from a demo control. Returns its aria-label or visible text.
// 参数：event 是演示控件的任意 React 事件。返回其 aria-label 或可见文字。
export function actionLabel(event: SyntheticEvent<Element> | Event): string {
  const target = event.currentTarget as HTMLElement | null
  return target?.getAttribute("aria-label") ?? target?.textContent ?? ""
}
