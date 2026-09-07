import { act } from "react"
import type { Root } from "react-dom/client"
import { expect, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeProfileLifetimes, VmmNativeProfilePolicySummary } from "@/types"
import { VmmNativeRuntimePolicy } from "./vmm-native-runtime-policy"
import { VmmNativeProjectProfilePolicy } from "./vmm-native-project-profile-policy"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"

// The explicitly enumerated expectations do not reuse a server reply or production lifetime calculator.
// 明确列出的预期不复用服务端响应或生产生命周期计算器。
const initial: VmmNativeProfilePolicySummary = {
  user: {
    transient: { baseMs: 86400011, stepMs: 3600013, maxMs: 90000041 },
    situational: { baseMs: 172800011, stepMs: 7200013, maxMs: 180000041 },
    stable: { baseMs: 259200011, stepMs: 10800013, maxMs: 270000041 },
  },
  project: {
    transient: { baseMs: 345600011, stepMs: 14400013, maxMs: 360000041 },
    situational: { baseMs: 432000011, stepMs: 18000013, maxMs: 450000041 },
    stable: { baseMs: 518400011, stepMs: 21600013, maxMs: 540000041 },
  },
}
// Every value changes in the second global generation; A's explicit project override must survive it.
// 第二代全局值全部变化；A 的显式项目覆盖必须在变化后保持。
const changed: VmmNativeProfilePolicySummary = {
  user: {
    transient: { baseMs: 604800011, stepMs: 25200013, maxMs: 630000041 },
    situational: { baseMs: 691200011, stepMs: 28800013, maxMs: 720000041 },
    stable: { baseMs: 777600011, stepMs: 32400013, maxMs: 810000041 },
  },
  project: {
    transient: { baseMs: 864000011, stepMs: 36000013, maxMs: 900000041 },
    situational: { baseMs: 950400011, stepMs: 39600013, maxMs: 990000041 },
    stable: { baseMs: 1036800011, stepMs: 43200013, maxMs: 1080000041 },
  },
}
// A's nine exact values differ from both generations of project defaults and all personal rules.
// A 的九个准确值区别于两代项目默认值及全部个人规则。
const override: VmmNativeProfileLifetimes = {
  transient: { baseMs: 1123200011, stepMs: 46800013, maxMs: 1170000041 },
  situational: { baseMs: 1209600011, stepMs: 50400013, maxMs: 1260000041 },
  stable: { baseMs: 1296000011, stepMs: 54000013, maxMs: 1350000041 },
}
// Only the real page's declared action signature is accepted.
// 仅接收真实页面声明的动作签名。
type RunAction = (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult>

// Runs four real page stages using the private HTTP facade and independent Rust consumer acknowledgments.
// 使用私有 HTTP 门面及独立 Rust 消费确认运行四个真实页面阶段。
// Takes the mounted test root and phase waiter; returns only after exact network response checks succeed.
// 接收已挂载测试根及阶段等待器；仅在准确网络响应检查通过后返回。
export async function verifyProfilePhases(container: HTMLElement, root: Root, runAction: RunAction, phase: (name: string) => Promise<void>): Promise<void> {
  // Responses are observations of actual service work, never synthetic fixture results.
  // 响应是实际服务工作的观察结果，绝不是合成夹具结果。
  const results: VmmMemoryActionResult[] = []
  // Forwards the exact optional payload and returns the production response unchanged.
  // 转发准确可选载荷并原样返回生产响应。
  const run: RunAction = async (action, payload) => {
    // The existing facade remains the sole response owner.
    // 既有门面仍是唯一响应归属方。
    const result = await runAction(action, payload)
    results.push(result)
    return result
  }
  // Waits for an exact acknowledged result count and settles React state.
  // 等待准确已确认结果数量并完成 React 状态更新。
  const wait = async (count: number): Promise<void> => {
    await act(async () => { await vi.waitFor(() => expect(results).toHaveLength(count), { timeout: 10000 }) })
  }
  // Edits one actual numeric input with its native setter and input event, not component state.
  // 使用原生设置器及输入事件编辑一个真实数值控件，不操作组件状态。
  const edit = async (id: string, value: number): Promise<void> => {
    // Each declared ID resolves to exactly one input in the current real page.
    // 每个声明 ID 在当前真实页面中准确解析为一个输入控件。
    const input = container.querySelector<HTMLInputElement>(`#${id}`)!
    expect(input).not.toBeNull()
    // The native setter makes React observe the same edit path as user input.
    // 原生设置器让 React 观察与用户输入相同的编辑路径。
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
    await act(async () => { setter.call(input, String(value)); input.dispatchEvent(new Event("input", { bubbles: true })) })
  }
  // Clicks one exact enabled save/inherit label and returns its acknowledged response.
  // 点击一个准确可用的保存／继承文案按钮并返回已确认响应。
  const click = async (label: string): Promise<VmmMemoryActionResult> => {
    // The real button must reflect a valid dirty draft or explicit inheritance action.
    // 真实按钮必须反映有效已编辑草稿或显式继承动作。
    const button = Array.from(container.querySelectorAll("button")).find((entry) => entry.textContent === label)!
    expect(button).toBeDefined()
    expect(button.disabled).toBe(false)
    // Record the pre-click count so duplicate requests cannot pass the assertion.
    // 记录点击前数量，使重复请求不能通过断言。
    const count = results.length + 1
    await act(async () => button.click())
    await wait(count)
    return results[count - 1]
  }
  await phase("profile-setup")
  // Obtain the actual catalog instead of inventing project summaries or numeric identities.
  // 获取实际目录，不虚构项目摘要或数字身份。
  const catalog = await run("native_project_catalog")
  if (catalog.data.kind !== "native_project_catalog") throw new Error("Expected real native project catalog")
  // Keep the exact discriminated catalog across render callbacks.
  // 跨渲染回调保留准确已判别目录。
  const projects = catalog.data.value.projects
  // Ownership is the stable host binding from the real catalog, never a display-name lookup.
  // 归属来自实际目录的稳定宿主绑定，绝不按显示名称查找。
  const owned = projects.filter((project) => project.ownerVulcanProjectIds.includes("http-profile-owner-a"))
  expect(owned).toHaveLength(1)
  // This ID is subsequently checked independently by the Rust runtime.
  // 此 ID 随后由 Rust 运行时独立检查。
  const project = owned[0]
  // Mounts a real page and waits for its own policy read, never passing a saved-policy response stub.
  // 挂载真实页面并等待其自行读取策略，绝不传入已保存策略响应桩。
  const mount = async (global: boolean): Promise<void> => {
    // Every mount must add one actual policy read.
    // 每次挂载必须增加一次实际策略读取。
    const count = results.length + 1
    await act(async () => root.render(null))
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN">{global
      ? <VmmNativeRuntimePolicy busy={false} onRunAction={run} />
      : <VmmNativeProjectProfilePolicy projects={projects} busy={false} onRunAction={run} />}</I18nProvider>))
    if (!global) await chooseMemoryOption(container.querySelector<HTMLButtonElement>("#profile-policy-project")!, `${project.name} · ${project.tag}`)
    await wait(count)
  }
  // Saves all eighteen global values through the role/level-owned controls in explicit milliseconds.
  // 通过角色／等级归属控件以明确毫秒保存全部十八个全局值。
  const saveGlobal = async (policy: VmmNativeProfilePolicySummary): Promise<void> => {
    await mount(true)
    for (const role of ["user", "project"] as const) for (const level of ["transient", "situational", "stable"] as const) {
      await chooseMemoryOption(container.querySelector<HTMLButtonElement>(`#profile-${role}-${level}-unit`)!, "毫秒")
      for (const field of ["baseMs", "stepMs", "maxMs"] as const) await edit(`profile-${role}-${level}-${field}`, policy[role][level][field])
    }
    // The saved whole group must exactly equal the independent expectation.
    // 保存后的整组必须准确等于独立预期。
    const result = await click("保存画像期限")
    if (result.data.kind !== "native_runtime_policy") throw new Error("Expected real global profile save")
    expect(result.data.value.profiles).toEqual(policy)
  }
  await saveGlobal(initial)
  await phase("profile-global")
  await mount(false)
  for (const level of ["transient", "situational", "stable"] as const) for (const field of ["baseMs", "stepMs", "maxMs"] as const) {
    // Project duration controls own one unit selector per field, unlike global row-shared units.
    // 项目时长控件每字段持有一个单位选择器，不同于全局行共享单位。
    const input = container.querySelector<HTMLInputElement>(`#project-profile-${level}-${field}`)!
    await chooseMemoryOption(input.parentElement!.querySelector<HTMLButtonElement>('[role="combobox"]')!, "毫秒")
    await edit(input.id, override[level][field])
  }
  // The actual project save must preserve the catalog identity and the whole override group.
  // 实际项目保存必须保留目录身份及完整覆盖组。
  const saved = await click("保存项目覆盖")
  if (saved.data.kind !== "native_project_profile_policy") throw new Error("Expected real project profile save")
  expect(saved.data.value.projectId).toBe(project.projectId)
  expect(saved.data.value.overrides).toEqual(override)
  expect(saved.data.value.effective).toEqual(override)
  await phase("profile-project")
  await saveGlobal(changed)
  await phase("profile-changed")
  await mount(false)
  // The next read must retain A's override even after every global field changed.
  // 即使全部全局字段变化，下一次读取仍须保留 A 的覆盖。
  const read = results[results.length - 1]
  if (read.data.kind !== "native_project_profile_policy") throw new Error("Expected actual project profile read")
  expect(read.data.value.overrides).toEqual(override)
  expect(read.data.value.global).toEqual(changed.project)
  // Inheritance is represented by absence of the override, not a copy of the defaults.
  // 继承表示为覆盖缺失，而非默认值的副本。
  const inherited = await click("恢复全局继承")
  if (inherited.data.kind !== "native_project_profile_policy") throw new Error("Expected actual profile inheritance")
  expect(inherited.data.value.projectId).toBe(project.projectId)
  expect(inherited.data.value.overrides).toBeNull()
  expect(inherited.data.value.effective).toEqual(changed.project)
  await phase("profile-inherit")
  expect(results.map((result) => result.action)).toEqual([
    "native_project_catalog", "native_runtime_policy", "native_profile_policy_update",
    "native_project_profile_policy", "native_project_profile_policy_update",
    "native_runtime_policy", "native_profile_policy_update",
    "native_project_profile_policy", "native_project_profile_policy_update",
  ])
}
