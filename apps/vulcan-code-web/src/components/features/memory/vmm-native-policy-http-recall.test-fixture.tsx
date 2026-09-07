import { act } from "react"
import type { Root } from "react-dom/client"
import { expect, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeRecallPolicySummary } from "@/types"
import { VmmNativeHostProjectPolicy } from "./vmm-native-host-project-policy"
import { VmmNativeRuntimePolicy } from "./vmm-native-runtime-policy"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"

// The global expectation enumerates all thirteen fields independently of the Rust response.
// 全局预期独立于 Rust 响应列出全部十三字段。
const globalPolicy: VmmNativeRecallPolicySummary = {
  precheckHistoryTurnLimit: 2, precheckMaxInputTokens: 6000, precheckRecallTopK: 2,
  precheckReviewCandidateLimit: 3, precheckMinReviewScore: 0, maxSearchQueries: 2,
  postactionHistoryTurnLimit: 2, postactionMaxInputTokens: 6000, postactionMemoryPriority: 73,
  postactionMemoryStability: "archival", postactionMemoryLevel: "persistent",
  postactionMemoryRefreshWeight: 9, postactionMemoryExpiryMs: 129600000,
}
// Host limits intentionally request more history than fits in their one-token budgets.
// 宿主上限有意请求超过其单令牌预算所容纳的历史。
const hostPolicy: VmmNativeRecallPolicySummary = {
  precheckHistoryTurnLimit: 3, precheckMaxInputTokens: 1, precheckRecallTopK: 3,
  precheckReviewCandidateLimit: 1, precheckMinReviewScore: 0, maxSearchQueries: 1,
  postactionHistoryTurnLimit: 3, postactionMaxInputTokens: 1, postactionMemoryPriority: 81,
  postactionMemoryStability: "working", postactionMemoryLevel: "phase",
  postactionMemoryRefreshWeight: 7, postactionMemoryExpiryMs: 216000000,
}
// The final global values independently exercise a review window below the recall limit.
// 最终全局值独立验证低于召回上限的评审窗口。
const windowPolicy: VmmNativeRecallPolicySummary = {
  precheckHistoryTurnLimit: 1, precheckMaxInputTokens: 4000, precheckRecallTopK: 3,
  precheckReviewCandidateLimit: 2, precheckMinReviewScore: 0, maxSearchQueries: 2,
  postactionHistoryTurnLimit: 1, postactionMaxInputTokens: 4000, postactionMemoryPriority: 62,
  postactionMemoryStability: "stable", postactionMemoryLevel: "stable",
  postactionMemoryRefreshWeight: 3, postactionMemoryExpiryMs: 259200000,
}
// The strict phase changes the score floor and opens the complete candidate review window.
// 严格阶段改变评分门槛，并打开完整候选评审窗口。
const strictPolicy: VmmNativeRecallPolicySummary = { ...windowPolicy, precheckReviewCandidateLimit: 3, precheckMinReviewScore: 0.99 }

// The action signature is the existing page contract connected to the real HTTP facade.
// 动作签名是连接真实 HTTP 门面的既有页面契约。
type RunAction = (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult>

// verifyRecallPhases takes a real root/facade and independent Rust phase waiter; returns after five stages.
// verifyRecallPhases 接收真实根／门面及独立 Rust 阶段等待器；五个阶段后返回。
export async function verifyRecallPhases(container: HTMLElement, root: Root, runAction: RunAction, phase: (name: string) => Promise<void>): Promise<void> {
  // Capture acknowledged responses only; no synthetic payload can satisfy an observation.
  // 仅捕获已确认响应；合成载荷不能满足观察。
  const results: VmmMemoryActionResult[] = []
  // run forwards the declared optional payload unchanged and returns the actual service response.
  // run 原样转发声明的可选载荷并返回实际服务响应。
  const run: RunAction = async (action, payload) => {
    // Only the real HTTP facade owns this response.
    // 仅真实 HTTP 门面持有此响应。
    const result = await runAction(action, payload)
    results.push(result)
    return result
  }
  // waitResult takes an exact response count and flushes React after live HTTP settles.
  // waitResult 接收准确响应数量，在实际 HTTP 完成后刷新 React。
  const waitResult = async (count: number): Promise<void> => {
    await act(async () => { await vi.waitFor(() => expect(results).toHaveLength(count), { timeout: 10000 }) })
  }
  // mount selects an actual page and forces its own initial read, including a preceding global page.
  // mount 选择实际页面并强制其自行初始读取，包括先前已是全局页面的情况。
  const mount = async (global: boolean): Promise<void> => {
    // The page must issue one read instead of reusing a sibling helper's stale component state.
    // 页面必须发出一次读取，不能复用相邻辅助函数的陈旧组件状态。
    const count = results.length + 1
    await act(async () => root.render(null))
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN">{global
      ? <VmmNativeRuntimePolicy busy={false} onRunAction={run} />
      : <VmmNativeHostProjectPolicy hostProjectId="replacement-host-a" busy={false} onRunAction={run} />}</I18nProvider>))
    await waitResult(count)
  }
  // save edits every field through DOM input/combobox events and checks the actual accepted whole group.
  // save 通过 DOM 输入／组合框事件编辑每个字段，并检查实际接纳整组。
  const save = async (policy: VmmNativeRecallPolicySummary, global: boolean): Promise<void> => {
    // Global and host pages declare distinct stable control prefixes.
    // 全局及宿主页面声明不同的稳定控制项前缀。
    const prefix = global ? "vmm-policy" : "caller-policy"
    // Choose milliseconds explicitly so loading another duration unit cannot change the intended value.
    // 显式选择毫秒，避免加载不同单位后改变目标值。
    const expiry = container.querySelector<HTMLInputElement>(`#${prefix}-postactionMemoryExpiryMs`)!
    await chooseMemoryOption(expiry.parentElement!.querySelector<HTMLButtonElement>('[role="combobox"]')!, "毫秒")
    for (const [key, value] of Object.entries(policy)) {
      if (typeof value !== "number") continue
      // Every numeric property has one declared input owner on this page.
      // 每个数值属性在此页面具有一个声明的输入归属。
      const input = container.querySelector<HTMLInputElement>(`#${prefix}-${key}`)!
      expect(input).not.toBeNull()
      // Use the native setter and event, not component state or action-payload replacement.
      // 使用原生设置器及事件，不使用组件状态或动作载荷替换。
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
      await act(async () => { setter.call(input, String(value)); input.dispatchEvent(new Event("input", { bubbles: true })) })
    }
    // Closed labels map exactly to the API's stability and memory-level choices.
    // 封闭文案准确映射到接口的稳定性及记忆等级选项。
    const stability = { working: "短期工作", stable: "稳定保留", archival: "归档保留" }
    // Session and phase are distinct domain levels, not numeric internal IDs.
    // 会话与阶段是不同领域等级，不是内部数字 ID。
    const level = { session: "会话级", phase: "阶段级", stable: "稳定级", persistent: "持久级" }
    await chooseMemoryOption(container.querySelector<HTMLButtonElement>(`#${prefix}-postactionMemoryStability`)!, stability[policy.postactionMemoryStability])
    await chooseMemoryOption(container.querySelector<HTMLButtonElement>(`#${prefix}-postactionMemoryLevel`)!, level[policy.postactionMemoryLevel])
    // The actual enabled button proves a dirty valid draft will be submitted.
    // 实际可用按钮证明将提交已修改且有效的草稿。
    const button = Array.from(container.querySelectorAll("button")).find((entry) => entry.textContent === "保存召回与提炼策略")!
    expect(button.disabled).toBe(false)
    // Each save must yield exactly one new acknowledged network response.
    // 每次保存必须恰好产生一个新增已确认网络响应。
    const count = results.length + 1
    await act(async () => button.click())
    await waitResult(count)
    // Discriminated response ownership must match the explicitly selected page.
    // 判别响应归属必须符合明确选择的页面。
    const result = results[count - 1]
    if (global) {
      if (result.data.kind !== "native_runtime_policy") throw new Error("Expected actual global recall response")
      expect(result.data.value.recall).toEqual(policy)
    } else {
      if (result.data.kind !== "native_host_project_policy") throw new Error("Expected actual host recall response")
      expect(result.data.value.overrides.recall).toEqual(policy)
      expect(result.data.value.effective.recall).toEqual(policy)
      expect(result.data.value.overrides.text).toBeNull()
      expect(result.data.value.overrides.retrieval).toBeNull()
    }
  }
  // Rust must finish normal seed creation before a UI save can reduce the shared direct-review budget.
  // Rust 必须完成正常种子创建后，界面保存才能降低共享主动评审预算。
  await phase("recall-setup")
  await mount(true)
  await save(globalPolicy, true)
  await phase("recall-global")
  await mount(false)
  await save(hostPolicy, false)
  await phase("recall-host")
  await mount(true)
  await save(strictPolicy, true)
  await phase("recall-strict")
  await mount(false)
  // Only the first, recall-owned control may restore this group to current global inheritance.
  // 仅第一个归属于召回的控制项可以将此组恢复为当前全局继承。
  const inheritance = Array.from(container.querySelectorAll("button")).filter((button) => button.textContent === "恢复本组全局继承")
  expect(inheritance).toHaveLength(3)
  expect(inheritance[0].disabled).toBe(false)
  await act(async () => inheritance[0].click())
  await waitResult(8)
  // The server must acknowledge absence of an override, not an override equal to current defaults.
  // 服务必须确认没有覆盖，而不是与当前默认值相等的覆盖。
  const inherited = results[7]
  if (inherited.data.kind !== "native_host_project_policy") throw new Error("Expected actual recall inheritance")
  expect(inherited.data.value.overrides.recall).toBeNull()
  expect(inherited.data.value.effective.recall).toEqual(strictPolicy)
  await phase("recall-inherit")
  await mount(true)
  await save(windowPolicy, true)
  await phase("recall-window")
  expect(results.map((result) => result.action)).toEqual([
    "native_runtime_policy", "native_runtime_policy_update", "native_host_project_policy", "native_host_project_policy_update",
    "native_runtime_policy", "native_runtime_policy_update", "native_host_project_policy", "native_host_project_policy_update",
    "native_runtime_policy", "native_runtime_policy_update",
  ])
}
