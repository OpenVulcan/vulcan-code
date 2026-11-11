// @vitest-environment jsdom

import { act, useState } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"

import { I18nProvider } from "@/i18n"
import { ProviderRetryRuleList, type ProviderRetryRuleEditor } from "./provider-retry-rule-list"

// Keeps the settings draft in a controlled harness so row actions affect only the targeted rule.
// 在受控测试容器中保存设置草稿，让每行操作仅影响指定规则。
function RuleListHarness() {
  // Retain a literal and regex together to check their independent identities.
  // 同时保留字面与正则规则，验证它们各自独立。
  const [rules, setRules] = useState(["temporary error", "regex:^deadline [0-9]+$"])
  // Mirror the settings page's open editor state.
  // 模拟设置页打开的编辑器状态。
  const [editor, setEditor] = useState<ProviderRetryRuleEditor | null>(null)
  return <ProviderRetryRuleList defaultRules={[]} editor={editor} onEditorChange={setEditor} onRulesChange={setRules} rules={rules} />
}

// Checks a single rule can be removed and a new entry shows an example tied to its input.
// 验证可删除单条规则，且新增条目的样例与其输入框关联。
it("removes an individual retry rule and displays an example when adding one", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true)
  // Mount on the document so the example can be found by its accessible description ID.
  // 挂载到文档，便于根据无障碍描述 ID 找到样例。
  const host = document.createElement("div")
  document.body.appendChild(host)
  const root = createRoot(host)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="en-US"><RuleListHarness /></I18nProvider>))
    await act(async () => host.querySelector<HTMLButtonElement>("li button:last-child")!.click())
    expect(Array.from(host.querySelectorAll("li code"), (node) => node.textContent)).toEqual(["^deadline [0-9]+$"])
    await act(async () => host.querySelector<HTMLButtonElement>("fieldset > .vk-retry-rule-actions > button")!.click())
    // The initial keyword mode displays a literal example rather than a regex pattern.
    // 初始关键词模式展示字面匹配样例。
    const input = host.querySelector("input")!
    const example = document.getElementById(input.getAttribute("aria-describedby")!)
    expect(example?.querySelector("code")?.textContent).toBe("connection reset")
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "gateway unreachable")
      input.dispatchEvent(new Event("input", { bubbles: true }))
    })
    await act(async () => host.querySelector<HTMLButtonElement>(".vk-retry-rule-editor button[type=button]:not([role=combobox])")!.click())
    expect(Array.from(host.querySelectorAll("li code"), (node) => node.textContent)).toEqual(["^deadline [0-9]+$", "gateway unreachable"])
  } finally {
    await act(async () => root.unmount())
    host.remove()
    vi.unstubAllGlobals()
  }
})
