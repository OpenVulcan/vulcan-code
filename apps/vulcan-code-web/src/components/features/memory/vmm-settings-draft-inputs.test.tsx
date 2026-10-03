// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench"
import type { VmmConfigDraftSummary } from "@/types"
import { VmmRulesPage } from "./vmm-rules-page"
import { VmmAdvancedPage } from "./vmm-advanced-page"
import { isOptionalUnsignedDraft } from "./vmm-draft-number"

// The UI grammar mirrors native optional u32/u64 parsing, including inheritance and exact boundaries.
// UI 文法对应原生可选 u32/u64 解析，包含继承及准确边界。
it("validates optional unsigned values without erasing punctuation or rounding u64", () => {
  for (const value of ["", "  ", "0", "+1", " 00042 ", "4294967295"]) expect(isOptionalUnsignedDraft(value, 32)).toBe(true)
  for (const value of ["-1", "1.5", "1e3", "4294967296", "Infinity", "1 2"]) expect(isOptionalUnsignedDraft(value, 32)).toBe(false)
  expect(isOptionalUnsignedDraft("18446744073709551615", 64)).toBe(true)
  expect(isOptionalUnsignedDraft("18446744073709551616", 64)).toBe(false)
  expect(isOptionalUnsignedDraft("0".repeat(10000) + "1", 64)).toBe(true)
})

/** Mounts one real settings page with a controlled synthetic draft and explicit callbacks.
 * 使用受控合成草稿和明确回调挂载一个真实设置页面。
 * page selects the rules or backend surface; returns its DOM, callbacks, editor and disposal.
 * page 选择规则或后端表面；返回 DOM、回调、编辑器和清理入口。
 */
async function mount(page: "rules" | "backend") {
  // The fallback factory provides the complete existing snapshot contract without network requests.
  // 预览工厂提供完整既有快照契约，不进行网络请求。
  const snapshot = browserFallbackSnapshot().vmmMemory
  let draft: VmmConfigDraftSummary = { ...snapshot.globalDraft, backendMode: "external_grpc", nativeProjectBindingMode: "per_project" }
  const container = document.createElement("div")
  const root = createRoot(container)
  const onSave = vi.fn(async () => true)
  const onApply = vi.fn(async () => true)
  const onTestManagement = vi.fn(async () => true)
  const onRunAction = vi.fn(async () => null)
  document.body.append(container)
  // Changes retain the exact user draft and are acknowledged by a controlled rerender.
  // 修改保留准确用户草稿，通过受控重渲染确认。
  const onChange = vi.fn((patch: Partial<VmmConfigDraftSummary>) => { draft = { ...draft, ...patch }; render() })
  const render = (busy = false) => root.render(<I18nProvider initialLocale="zh-CN">{page === "rules" ? <VmmRulesPage busy={busy} scope="global" projectId="" draftReady draft={draft} projects={snapshot.projectScopes} notice="" onChange={onChange} onApply={onApply} onSelectScope={() => {}} /> : <VmmAdvancedPage busy={busy} draft={draft} snapshot={snapshot} notice="" onChange={onChange} onSave={onSave} onTestManagement={onTestManagement} onRunAction={onRunAction} />}</I18nProvider>)
  await act(async () => render())
  // edit exercises the browser input event associated with the exact draft value.
  // edit 触发对应准确草稿值的浏览器输入事件。
  const edit = async (current: string, next: string) => {
    const input = Array.from(container.querySelectorAll("input")).find((entry) => entry.value === current)!
    expect(input).toBeDefined()
    await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, next); input.dispatchEvent(new Event("input", { bubbles: true })) })
  }
  return { container, onChange, onSave, onApply, onRunAction, edit, snapshot,
    render: async (busy: boolean) => { await act(async () => render(busy)) },
    dispose: async () => { await act(async () => root.unmount()); container.remove() } }
}

// Editing fractional counts must expose the invalid draft rather than silently multiplying it by ten.
// 编辑小数次数必须显示无效原始草稿，不能静默变成十倍整数。
it("keeps fractional rule input intact and blocks saving until corrected or inherited", async () => {
  const view = await mount("rules")
  try {
    await view.edit(view.snapshot.globalDraft.implicitMemoryTurns, "1.5")
    expect(view.onChange).toHaveBeenLastCalledWith({ implicitMemoryTurns: "1.5" })
    expect(view.container.querySelector('[aria-invalid="true"]')).not.toBeNull()
    // The apply action is located by its label because the scope selector also renders a button before it.
    // 应用操作按文案定位，因为作用域选择器也会在其之前渲染按钮。
    const applyButton = () => Array.from(view.container.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent === "应用设置")!
    expect(applyButton().disabled).toBe(true)
    expect(view.onApply).not.toHaveBeenCalled()
    await view.edit("1.5", "")
    expect(view.container.querySelector('[role="alert"]')).toBeNull()
    await act(async () => applyButton().click())
    expect(view.onApply.mock.calls[0]).toEqual(["global", null, expect.objectContaining({ implicitMemoryTurns: "" })])
  } finally { await view.dispose() }
})

// Inactive external transport errors must not block native settings or leak external credential fields.
// 非活动外链传输错误不得阻塞原生设置，也不得泄露外链凭据字段。
it("retains invalid transport text and isolates it when selecting the native backend", async () => {
  const view = await mount("backend")
  try {
    // Expand the actual transport accordion before editing its timeout draft.
    // 编辑超时草稿前展开真实传输折叠区。
    await act(async () => Array.from(view.container.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent === "连接高级参数")!.click())
    await view.edit(view.snapshot.globalDraft.managementTimeoutMs, "-1")
    expect(view.onChange).toHaveBeenLastCalledWith({ managementTimeoutMs: "-1" })
    expect(view.container.querySelector<HTMLButtonElement>("button")!.disabled).toBe(true)
    expect(Array.from(view.container.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent === "保存并测试管理连接")!.disabled).toBe(true)
    expect(view.container.querySelectorAll('input[type="password"]')).toHaveLength(2)
    await act(async () => view.container.querySelector<HTMLButtonElement>('[role="radio"][value="native"]')!.click())
    expect(view.container.querySelectorAll('input[type="password"]')).toHaveLength(0)
    expect(view.container.querySelector('p[role="alert"]')).toBeNull()
    expect(view.container.querySelector<HTMLButtonElement>("button")!.disabled).toBe(false)
    await view.render(true)
    expect(Array.from(view.container.querySelectorAll<HTMLButtonElement>('[role="radio"], [role="combobox"]')).every((button) => button.disabled)).toBe(true)
    expect(view.onSave).not.toHaveBeenCalled()
  } finally { await view.dispose() }
})
