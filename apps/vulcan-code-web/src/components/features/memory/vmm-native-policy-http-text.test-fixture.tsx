import { act } from "react"
import type { Root } from "react-dom/client"
import { expect, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeTextPolicySummary } from "@/types"
import { VmmNativeHostProjectPolicy } from "./vmm-native-host-project-policy"
import { VmmNativeRuntimePolicy } from "./vmm-native-runtime-policy"

// RunAction forwards the existing component contract to the actual private HTTP facade.
// RunAction 将既有组件契约转发到实际私有 HTTP 门面。
type RunAction = (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult>

// verifyTextPhases takes the mounted root, real action facade and Rust phase waiter; returns after five saves.
// verifyTextPhases 接收挂载根、真实动作门面及 Rust 阶段等待器；在五次保存后返回。
export async function verifyTextPhases(container: HTMLElement, root: Root, runAction: RunAction, phase: (name: string) => Promise<void>): Promise<void> {
  // Only acknowledged network responses enter this observation list; no response values are fabricated.
  // 仅已确认网络响应进入此观察列表；不伪造响应值。
  const results: VmmMemoryActionResult[] = []
  // Independent desired groups isolate the two declared text controls.
  // 独立目标组隔离两个声明的文本控制项。
  const folded: VmmNativeTextPolicySummary = { foldMachineText: true, maxFragmentTokens: 20000 }
  // The host differs in folding alone while retaining the same budget.
  // 宿主仅折叠不同，保留相同预算。
  const retained: VmmNativeTextPolicySummary = { foldMachineText: false, maxFragmentTokens: 20000 }
  // The global clipped policy differs only in budget relative to folded.
  // 相对折叠策略，全局裁剪策略仅预算不同。
  const clipped: VmmNativeTextPolicySummary = { foldMachineText: true, maxFragmentTokens: 256 }
  // run observes actual responses from the supplied real facade and returns them unchanged.
  // run 观察所供真实门面的实际响应并原样返回。
  const run: RunAction = async (action, payload) => {
    // Preserve the exact response identity and all sibling groups.
    // 保留准确响应身份和全部相邻组。
    const result = await runAction(action, payload)
    results.push(result)
    return result
  }
  // waitResult takes an exact response count, flushing React after the asynchronous HTTP callback settles.
  // waitResult 接收准确响应数量，在异步 HTTP 回调完成后刷新 React。
  const waitResult = async (count: number): Promise<void> => {
    await act(async () => { await vi.waitFor(() => expect(results).toHaveLength(count), { timeout: 10000 }) })
  }
  // mount selects a real global or host page; both load their own actual service state.
  // mount 选择真实全局或宿主页；两者均自行加载实际服务状态。
  const mount = async (global: boolean): Promise<void> => {
    // Each page's one initial read must be observable, not synthesized by this helper.
    // 每个页面唯一的初始读取必须可观察，不能由此辅助函数合成。
    const count = results.length + 1
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN">{global
      ? <VmmNativeRuntimePolicy busy={false} onRunAction={run} />
      : <VmmNativeHostProjectPolicy hostProjectId="replacement-host-a" busy={false} onRunAction={run} />}</I18nProvider>))
    await waitResult(count)
    expect(container.querySelector<HTMLInputElement>("#vmm-text-budget")).not.toBeNull()
  }
  // save takes the desired group and expected ownership, drives actual DOM controls and checks the real reply.
  // save 接收目标组及预期归属，驱动实际 DOM 控制项并检查真实回复。
  const save = async (policy: VmmNativeTextPolicySummary, global: boolean): Promise<void> => {
    // This native setter emits the component's declared edit event without accessing React state.
    // 此原生设置器发出组件声明的编辑事件，不访问 React 状态。
    const input = container.querySelector<HTMLInputElement>("#vmm-text-budget")!
    // Exact browser property setter for the existing controlled input.
    // 既有受控输入的准确浏览器属性设置器。
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
    await act(async () => { setter.call(input, String(policy.maxFragmentTokens)); input.dispatchEvent(new Event("input", { bubbles: true })) })
    // The exact localized label uniquely identifies text folding among sibling Boolean groups.
    // 准确本地化文案在相邻布尔组中唯一识别文本折叠。
    const label = Array.from(container.querySelectorAll("label")).find((entry) => entry.textContent === "折叠长代码与机器日志")!
    expect(label).toBeDefined()
    // A switch changes only when its actual acknowledged state differs from the desired Boolean.
    // 仅在实际已确认状态与目标布尔值不同时改变开关。
    const toggle = label.querySelector<HTMLButtonElement>('[role="switch"]')!
    if (toggle.getAttribute("aria-checked") !== String(policy.foldMachineText)) await act(async () => toggle.click())
    expect(toggle.getAttribute("aria-checked")).toBe(String(policy.foldMachineText))
    // The enabled save proves an actual dirty draft exists, even when only one control changed.
    // 可用保存按钮证明实际存在已修改草稿，即使仅一个控制项变化。
    const button = Array.from(container.querySelectorAll("button")).find((entry) => entry.textContent === "保存文本清洗设置")!
    expect(button.disabled).toBe(false)
    // Every click must add exactly one acknowledged service response.
    // 每次点击必须恰好增加一个已确认服务响应。
    const count = results.length + 1
    await act(async () => button.click())
    await waitResult(count)
    // Validate the discriminated production envelope according to the explicitly chosen page.
    // 根据明确选择的页面验证生产判别封套。
    const result = results[count - 1]
    if (global) {
      if (result.data.kind !== "native_runtime_policy") throw new Error("Expected actual global text response")
      expect(result.data.value.text).toEqual(policy)
    } else {
      if (result.data.kind !== "native_host_project_policy") throw new Error("Expected actual host text response")
      expect(result.data.value.overrides.text).toEqual(policy)
      expect(result.data.value.effective.text).toEqual(policy)
      expect(result.data.value.overrides.recall).toBeNull()
      expect(result.data.value.overrides.retrieval).toBeNull()
    }
    expect(input.value).toBe(String(policy.maxFragmentTokens))
  }
  await mount(true)
  await save(folded, true)
  await phase("text-global")
  await mount(false)
  await save(retained, false)
  await phase("text-host")
  await mount(true)
  await save(clipped, true)
  await phase("text-clipped")
  await mount(false)
  // Restore only the third, text-owned inheritance control; recall and retrieval remain untouched.
  // 仅恢复第三个归属于文本的继承控制项；召回及检索保持不变。
  const inheritance = Array.from(container.querySelectorAll("button")).filter((button) => button.textContent === "恢复本组全局继承")
  expect(inheritance).toHaveLength(3)
  expect(inheritance[2].disabled).toBe(false)
  await act(async () => inheritance[2].click())
  await waitResult(8)
  // The restored response must remove the override and project current global values into the editor.
  // 恢复响应必须移除覆盖，并将当前全局值投影到编辑器。
  const inherited = results[7]
  if (inherited.data.kind !== "native_host_project_policy") throw new Error("Expected actual text inheritance response")
  expect(inherited.data.value.overrides.text).toBeNull()
  expect(inherited.data.value.effective.text).toEqual(clipped)
  expect(container.querySelector<HTMLInputElement>("#vmm-text-budget")!.value).toBe("256")
  await phase("text-inherit")
  await mount(true)
  await save(folded, true)
  await phase("text-final")
  expect(results.map((result) => result.action)).toEqual([
    "native_runtime_policy", "native_text_policy_update", "native_host_project_policy", "native_host_project_policy_update",
    "native_runtime_policy", "native_text_policy_update", "native_host_project_policy", "native_host_project_policy_update",
    "native_runtime_policy", "native_text_policy_update",
  ])
}
