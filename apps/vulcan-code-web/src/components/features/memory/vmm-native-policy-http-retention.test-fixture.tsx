import { act } from "react"
import type { Root } from "react-dom/client"
import { expect, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeProjectRetentionRules, VmmNativeRetentionSettingsSummary } from "@/types"
import { VmmNativeRuntimePolicy } from "./vmm-native-runtime-policy"
import { VmmNativeProjectRetention } from "./vmm-native-project-retention"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"

// Three independent whole global expectations cover every editable value and both switches.
// 三个独立完整全局预期覆盖每个可编辑值及两个开关。
const globals: VmmNativeRetentionSettingsSummary[] = [
  { enabled: false, scanIntervalMinutes: 7, profileExpiryIntervalSeconds: 11, turnHotWindowSize: 3,
    sessionIdleMs: 259200000, trashRetentionMs: 172800000, protectedPriorityFloor: 50, protectedMemoryLevelFloor: 3, skipProtectedSharedMemories: true },
  { enabled: true, scanIntervalMinutes: 2, profileExpiryIntervalSeconds: 3, turnHotWindowSize: 1,
    sessionIdleMs: 86400000, trashRetentionMs: 86400000, protectedPriorityFloor: 100, protectedMemoryLevelFloor: 4, skipProtectedSharedMemories: true },
  { enabled: false, scanIntervalMinutes: 3, profileExpiryIntervalSeconds: 5, turnHotWindowSize: 4,
    sessionIdleMs: 259200000, trashRetentionMs: 345600000, protectedPriorityFloor: 0, protectedMemoryLevelFloor: 1, skipProtectedSharedMemories: false },
]
// Six memory-project fields exclude all global scheduler controls by contract.
// 六个记忆项目字段按契约排除全部全局调度控件。
const override: VmmNativeProjectRetentionRules = { turnHotWindowSize: 2, sessionIdleMs: 86400000,
  trashRetentionMs: 259200000, protectedPriorityFloor: 75, protectedMemoryLevelFloor: 4, skipProtectedSharedMemories: false }
// The action signature preserves the page's declared optional payload.
// 动作签名保留页面声明的可选载荷。
type RunAction = (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult>

// Drives data-retention and real scheduler saves through HTTP; Rust independently checks cleanup and timer effects.
// 通过 HTTP 驱动数据保留与真实调度保存；Rust 独立检查清洗及计时效果。
// Takes the real root, action facade and bounded phase waiter; returns after twenty-one exact responses.
// 接收真实根、动作门面及有界阶段等待器；准确二十一个响应后返回。
export async function verifyRetentionPhases(container: HTMLElement, root: Root, runAction: RunAction, phase: (name: string) => Promise<void>): Promise<void> {
  // These responses are observations only and cannot substitute for a real service response.
  // 这些响应仅用于观察，不能替代真实服务响应。
  const results: VmmMemoryActionResult[] = []
  // Forwards unchanged optional arguments and returns the acknowledged production response.
  // 原样转发可选参数并返回已确认生产响应。
  const run: RunAction = async (action, payload) => {
    // Only the real HTTP facade owns the result.
    // 仅真实 HTTP 门面持有结果。
    const result = await runAction(action, payload)
    results.push(result)
    return result
  }
  // Waits for the exact number of network acknowledgments and flushes React updates.
  // 等待准确网络确认数量并完成 React 更新。
  const wait = async (count: number): Promise<void> => {
    await act(async () => { await vi.waitFor(() => expect(results).toHaveLength(count), { timeout: 10000 }) })
  }
  // Sets one actual labeled switch only when its current state differs from the independent expectation.
  // 仅在当前状态不同于独立预期时设置一个实际带标签开关。
  const toggle = async (label: string, checked: boolean): Promise<void> => {
    // Exact visible label ownership avoids choosing switches in unrelated sibling policy cards.
    // 准确可见标签归属避免选择无关相邻策略卡的开关。
    const control = Array.from(container.querySelectorAll("label")).find((item) => item.textContent === label)!.querySelector<HTMLButtonElement>('[role="switch"]')!
    expect(control).not.toBeNull()
    expect(control.disabled).toBe(false)
    if ((control.getAttribute("aria-checked") === "true") !== checked) await act(async () => control.click())
    expect(control.getAttribute("aria-checked")).toBe(String(checked))
  }
  // Edits one numeric control through its native setter, including explicit millisecond unit selection.
  // 通过原生设置器编辑一个数值控件，包括显式选择毫秒单位。
  const edit = async (prefix: string, key: string, value: number): Promise<void> => {
    // This exact ID is declared by the selected global or memory-project component.
    // 此准确 ID 由选定的全局或记忆项目组件声明。
    const input = container.querySelector<HTMLInputElement>(`#${prefix}-${key}`)!
    expect(input).not.toBeNull()
    if (key === "sessionIdleMs" || key === "trashRetentionMs") {
      await chooseMemoryOption(input.parentElement!.querySelector<HTMLButtonElement>('[role="combobox"]')!, "毫秒")
    }
    // Native DOM input events keep production validation and dirty-state logic active.
    // 原生 DOM 输入事件保持生产校验及草稿变更逻辑参与。
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
    await act(async () => { setter.call(input, String(value)); input.dispatchEvent(new Event("input", { bubbles: true })) })
  }
  // Clicks an enabled exact label and waits for precisely one real reply.
  // 点击可用准确文案按钮并等待恰好一个真实回复。
  const click = async (label: string): Promise<VmmMemoryActionResult> => {
    // Each selected page owns one save/inheritance button with this exact localized label.
    // 每个选定页面持有一个使用此准确本地化文案的保存／继承按钮。
    const button = Array.from(container.querySelectorAll("button")).find((item) => item.textContent === label)!
    expect(button).toBeDefined()
    expect(button.disabled).toBe(false)
    // An exact response count rejects both missing and duplicate submissions.
    // 准确响应数量同时拒绝缺失及重复提交。
    const count = results.length + 1
    await act(async () => button.click())
    await wait(count)
    return results[count - 1]
  }
  await phase("retention-setup")
  // The catalog is read from the actual production service rather than a handcrafted project array.
  // 从实际生产服务读取目录，而非手工构造项目数组。
  const catalog = await run("native_project_catalog")
  if (catalog.data.kind !== "native_project_catalog") throw new Error("Expected real retention catalog")
  // Keep the narrowed real catalog across render callbacks.
  // 跨渲染回调保留已判别真实目录。
  const projects = catalog.data.value.projects
  // Stable binding identity, not a display name, chooses the owner to edit.
  // 通过稳定绑定身份选择编辑归属方，不使用显示名称。
  const owned = projects.filter((project) => project.ownerVulcanProjectIds.includes("http-retention-a"))
  expect(owned).toHaveLength(1)
  // This ID is checked independently against Rust's actual creation receipt.
  // 此 ID 独立对照 Rust 实际创建回执检查。
  const project = owned[0]
  // Mounts the actual chosen page and waits for its own initial policy read.
  // 挂载实际选定页面并等待其自行初始读取策略。
  const mount = async (global: boolean): Promise<void> => {
    // A forced unmount prevents prior-page state from replacing the next production read.
    // 强制卸载防止前一页面状态替代下一次生产读取。
    const count = results.length + 1
    await act(async () => root.render(null))
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN">{global
      ? <VmmNativeRuntimePolicy busy={false} onRunAction={run} />
      : <VmmNativeProjectRetention projects={projects} busy={false} onRunAction={run} />}</I18nProvider>))
    if (!global) await chooseMemoryOption(container.querySelector<HTMLButtonElement>("#retention-project")!, `${project.name} · ${project.tag}`)
    await wait(count)
  }
  // Changes all six business fields using their real numeric, named-level and switch controls.
  // 使用真实数值、具名等级及开关控件修改全部六个业务字段。
  const editRules = async (prefix: string, rules: VmmNativeProjectRetentionRules): Promise<void> => {
    for (const key of ["turnHotWindowSize", "sessionIdleMs", "trashRetentionMs", "protectedPriorityFloor"] as const) await edit(prefix, key, rules[key])
    // The API's four declared levels map to exact existing UI labels.
    // 接口声明的四个等级映射到准确既有界面文案。
    const levels = ["会话级", "阶段级", "稳定级", "持久级"]
    await chooseMemoryOption(container.querySelector<HTMLButtonElement>(`#${prefix}-protectedMemoryLevelFloor`)!, levels[rules.protectedMemoryLevelFloor - 1])
    await toggle("保护重要的共享终态记忆", rules.skipProtectedSharedMemories)
  }
  // Saves a whole global generation and validates the actual persisted response.
  // 保存完整全局代次并验证实际持久化响应。
  const saveGlobal = async (policy: VmmNativeRetentionSettingsSummary): Promise<void> => {
    await mount(true)
    // Global protection controls live in this exact advanced disclosure, which a user must open first.
    // 全局保护控件位于此准确高级折叠区，用户必须先展开。
    const details = container.querySelector("#vmm-cleanup-protectedPriorityFloor")!.closest("details")!
    if (!details.open) await act(async () => details.querySelector("summary")!.click())
    await toggle("启用定期清洗与画像过期扫描", policy.enabled)
    await edit("vmm-cleanup", "scanIntervalMinutes", policy.scanIntervalMinutes)
    await edit("vmm-cleanup", "profileExpiryIntervalSeconds", policy.profileExpiryIntervalSeconds)
    await editRules("vmm-cleanup", policy)
    // Require the entire accepted group, including the global-only scheduler controls.
    // 要求完整接纳组，包括仅全局存在的调度控件。
    const result = await click("保存清洗设置")
    if (result.data.kind !== "native_runtime_policy") throw new Error("Expected real retention save")
    expect(result.data.value.retention).toEqual(policy)
  }
  await saveGlobal(globals[0])
  await phase("retention-global")
  await mount(false)
  await editRules("project-retention", override)
  // The saved project identity must match the selected catalog receipt.
  // 保存项目身份必须匹配选定目录回执。
  const saved = await click("保存项目覆盖")
  if (saved.data.kind !== "native_project_retention") throw new Error("Expected actual owner retention save")
  expect(saved.data.value.projectId).toBe(project.projectId)
  expect(saved.data.value.overrides).toEqual(override)
  expect(saved.data.value.effective).toEqual(override)
  await phase("retention-project")
  await saveGlobal(globals[1])
  await phase("retention-changed")
  await mount(false)
  // Explicit inheritance removes the override and loads the latest global business rules.
  // 显式继承移除覆盖并加载最新全局业务规则。
  const inherited = await click("恢复全局继承")
  if (inherited.data.kind !== "native_project_retention") throw new Error("Expected actual retention inheritance")
  expect(inherited.data.value.projectId).toBe(project.projectId)
  expect(inherited.data.value.overrides).toBeNull()
  expect(inherited.data.value.effective).toEqual(inherited.data.value.global)
  await phase("retention-inherit")
  await saveGlobal(globals[2])
  await phase("retention-unprotected")
  expect(results.map((result) => result.action)).toEqual([
    "native_project_catalog", "native_runtime_policy", "native_retention_settings_update",
    "native_project_retention", "native_project_retention_update", "native_runtime_policy", "native_retention_settings_update",
    "native_project_retention", "native_project_retention_update", "native_runtime_policy", "native_retention_settings_update",
  ])
  await phase("schedule-setup")
  // Actual page saves change only scheduler fields; data-retention rules remain the final independent generation.
  // 实际页面保存仅改变调度字段；数据保留规则保持最后的独立代次。
  const schedules = [
    { phase: "schedule-enabled", enabled: true, scanIntervalMinutes: 1, profileExpiryIntervalSeconds: 1 },
    { phase: "schedule-lengthened", enabled: true, scanIntervalMinutes: 2, profileExpiryIntervalSeconds: 180 },
    { phase: "schedule-shortened", enabled: true, scanIntervalMinutes: 1, profileExpiryIntervalSeconds: 1 },
    { phase: "schedule-disabled", enabled: false, scanIntervalMinutes: 1, profileExpiryIntervalSeconds: 1 },
    { phase: "schedule-restored", enabled: false, scanIntervalMinutes: 3, profileExpiryIntervalSeconds: 5 },
  ]
  for (const schedule of schedules) {
    await saveGlobal({ ...globals[2], enabled: schedule.enabled, scanIntervalMinutes: schedule.scanIntervalMinutes,
      profileExpiryIntervalSeconds: schedule.profileExpiryIntervalSeconds })
    await phase(schedule.phase)
  }
  expect(results.slice(11).map((result) => result.action)).toEqual(
    Array.from({ length: 5 }, () => ["native_runtime_policy", "native_retention_settings_update"]).flat(),
  )
}
