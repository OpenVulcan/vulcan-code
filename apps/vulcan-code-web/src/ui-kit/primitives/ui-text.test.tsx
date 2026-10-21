// @vitest-environment jsdom
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, expect, it } from "vitest"
import { I18nProvider, useI18n } from "@/i18n"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogTitle } from "./dialog"
import { Sheet, SheetContent, SheetDescription, SheetTitle } from "./sheet"
import { Spinner } from "./spinner"
import { Toaster } from "./toaster"

// root and container own a real portal-capable React tree for each language case.
// root 与 container 为每个语言用例持有真实且支持门户的 React 树。
let root: Root
let container: HTMLDivElement
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
  container = document.createElement("div"); document.body.appendChild(container); root = createRoot(container)
})
afterEach(async () => { await act(async () => root.unmount()); container.remove() })

// LanguageSwitch uses the actual application authority to update the language without remounting the open surface.
// LanguageSwitch 使用实际应用权威切换语言，不重新挂载已打开的表面。
// Returns: two in-surface test controls for the supported application locales.
// 返回值：位于表面内、对应受支持应用语言的两个测试控件。
function LanguageSwitch() {
  const { setLocale } = useI18n()
  return <><button type="button" data-language="en-US" onClick={() => setLocale("en-US")}>English</button>
    <button type="button" data-language="zh-CN" onClick={() => setLocale("zh-CN")}>中文</button></>
}

// Surface renders real dialog/footer or sheet close controls, optionally alongside the actual locale switch.
// Surface 渲染真实弹窗及底栏或抽屉关闭控件，并可加入实际语言切换器。
// Parameters: kind selects the primitive; switchable enables application-only controls. Returns: an initially open modal.
// 参数：kind 选择基础组件；switchable 启用应用专属控件。返回值：首次处于打开状态的模态框。
function Surface({ kind, switchable = false }: { kind: "dialog" | "sheet"; switchable?: boolean }) {
  return kind === "dialog" ? <Dialog defaultOpen><DialogContent><DialogTitle>Dialog</DialogTitle><DialogDescription>Description</DialogDescription>
    {switchable ? <LanguageSwitch /> : null}<DialogFooter showCloseButton /></DialogContent></Dialog>
    : <Sheet defaultOpen><SheetContent><SheetTitle>Sheet</SheetTitle><SheetDescription>Description</SheetDescription>
      {switchable ? <LanguageSwitch /> : null}</SheetContent></Sheet>
}

// closeLabels reads only primitive-owned standard actions, excluding unrelated test and caller buttons.
// closeLabels 仅读取基础组件自有标准操作，排除无关测试和调用方按钮。
// Returns: visible and screen-reader close labels from the current portal.
// 返回值：当前门户中的可见及读屏关闭文案。
function closeLabels() {
  return [...document.querySelectorAll('[data-slot="dialog-close"], [data-slot="dialog-footer"] button, [data-slot="sheet-close"]')].map((button) => button.textContent)
}

it.each(["dialog", "sheet"] as const)("keeps %s independently mountable with English standard close actions", async (kind) => {
  await act(async () => root.render(<Surface kind={kind} />))
  expect(closeLabels()).toEqual(kind === "dialog" ? ["Close", "Close"] : ["Close"])
})

it.each([
  { kind: "dialog" as const, locale: "zh-CN", label: "关闭" }, { kind: "sheet" as const, locale: "zh-CN", label: "关闭" },
  { kind: "dialog" as const, locale: "en-US", label: "Close" }, { kind: "sheet" as const, locale: "en-US", label: "Close" },
])("localizes $kind in $locale and preserves its actual close action", async ({ kind, locale, label }) => {
  await act(async () => root.render(<I18nProvider initialLocale={locale}><Surface kind={kind} /></I18nProvider>))
  expect(closeLabels()).toEqual(kind === "dialog" ? [label, label] : [label])
  await act(async () => document.querySelector<HTMLButtonElement>(kind === "dialog" ? '[data-slot="dialog-footer"] button' : '[data-slot="sheet-close"]')!.click())
  expect(document.querySelector('[role="dialog"]')).toBeNull()
})

it.each(["dialog", "sheet"] as const)("updates every open %s close label when the application language changes", async (kind) => {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><Surface kind={kind} switchable /></I18nProvider>))
  const surface = document.querySelector('[role="dialog"]')
  await act(async () => document.querySelector<HTMLButtonElement>('[data-language="en-US"]')!.click())
  expect(closeLabels()).toEqual(kind === "dialog" ? ["Close", "Close"] : ["Close"])
  expect(document.querySelector('[role="dialog"]')).toBe(surface)
  await act(async () => document.querySelector<HTMLButtonElement>('[data-language="zh-CN"]')!.click())
  expect(closeLabels()).toEqual(kind === "dialog" ? ["关闭", "关闭"] : ["关闭"])
})

it("keeps the standalone loading indicator independent of application providers", async () => {
  await act(async () => root.render(<Spinner />))
  expect(container.querySelector('[role="status"]')?.getAttribute("aria-label")).toBe("Loading")
})

// The toast viewport follows the same live language authority without replacing explicit labels.
// 通知视口遵循相同的实时语言权威，不替换显式标签。
it("updates the real toast viewport label when the application language changes", async () => {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><Toaster /><LanguageSwitch /></I18nProvider>))
  const region = document.querySelector('section[aria-live="polite"]')!
  expect(region.getAttribute("aria-label")).toBe("通知")
  await act(async () => container.querySelector<HTMLButtonElement>('[data-language="en-US"]')!.click())
  expect(region.getAttribute("aria-label")).toBe("Notifications")
  expect(document.querySelector('section[aria-live="polite"]')).toBe(region)
  await act(async () => root.render(<Toaster containerAriaLabel="Specific updates" />))
  expect(document.querySelector('section[aria-live="polite"]')!.getAttribute("aria-label")).toBe("Specific updates")
})

it("updates loading labels with the application locale without replacing caller-owned descriptions", async () => {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><Spinner data-testid="default-spinner" /><Spinner data-testid="specific-spinner" aria-label="Reading catalog" /><LanguageSwitch /></I18nProvider>))
  // indicator is the same mounted SVG across locale updates; explicit labels remain authoritative.
  // indicator 在语言更新期间保持同一挂载 SVG；显式标签保持权威。
  const indicator = container.querySelector('[data-testid="default-spinner"]')!
  expect(indicator.getAttribute("aria-label")).toBe("加载中")
  await act(async () => container.querySelector<HTMLButtonElement>('[data-language="en-US"]')!.click())
  expect(indicator.getAttribute("aria-label")).toBe("Loading")
  expect(container.querySelector('[data-testid="default-spinner"]')).toBe(indicator)
  expect(container.querySelector('[data-testid="specific-spinner"]')!.getAttribute("aria-label")).toBe("Reading catalog")
  await act(async () => container.querySelector<HTMLButtonElement>('[data-language="zh-CN"]')!.click())
  expect(indicator.getAttribute("aria-label")).toBe("加载中")
})
