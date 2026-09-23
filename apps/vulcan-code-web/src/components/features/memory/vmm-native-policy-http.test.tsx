// @vitest-environment jsdom

import { access, readFile, writeFile } from "node:fs/promises"
import { isAbsolute, join } from "node:path"
import process from "node:process"
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { clearBrowserServiceEndpointCache } from "@/service/browser-service-discovery"
import { runVmmMemoryAction } from "@/service/memory-actions-service"
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime"
import type { VmmMemoryActionResult, VmmNativeRetrievalPolicySummary } from "@/types"
import { VmmNativeHostProjectPolicy } from "./vmm-native-host-project-policy"
import { VmmNativeRetrievalPolicy } from "./vmm-native-retrieval-policy"
import { verifyTextPhases } from "./vmm-native-policy-http-text.test-fixture"
import { verifyRecallPhases } from "./vmm-native-policy-http-recall.test-fixture"
import { verifyProfilePhases } from "./vmm-native-policy-http-profile.test-fixture"
import { verifyRetentionPhases } from "./vmm-native-policy-http-retention.test-fixture"

// The explicit runner owns the handshake directory; ordinary unit suites never open a service.
// 显式运行器持有握手目录；常规单元测试绝不打开服务。
const handshakeDirectory = process.env.VULCAN_VMM_HTTP_ACCEPTANCE_DIR

// Independently declared expectations are not server response stubs or values read from Rust output.
// 独立声明的预期值不是服务响应桩，也不是从 Rust 输出读取的值。
const globalPolicy: VmmNativeRetrievalPolicySummary = {
  hybridEnabled: false, lexicalCandidateFloor: 3, rrfK: "9223372036854775807",
  replaceMinSimilarity: 0.86, hardDedupeCosineThreshold: 0, hardDedupePoolTopK: 3,
  replacementTopK: 2, rerankTopN: 0, mmrEnabled: false, mmrLambda: 0.25,
  weibullEnabled: false, weibullShape: 2, weibullScaleHours: 0.5,
  weibullMinMultiplier: 0.25, weibullReinforceWeight: 0.5, weibullCrossSessionBoost: 0.75,
}

// Every caller field differs from the global group, including three explicit Boolean switches.
// 每个调用方字段均与全局组不同，包含三个显式布尔开关。
const hostPolicy: VmmNativeRetrievalPolicySummary = {
  hybridEnabled: true, lexicalCandidateFloor: 1, rrfK: "1", replaceMinSimilarity: 1,
  hardDedupeCosineThreshold: 0.55, hardDedupePoolTopK: 4, replacementTopK: 1,
  rerankTopN: 2, mmrEnabled: true, mmrLambda: 0, weibullEnabled: true,
  weibullShape: 0.75, weibullScaleHours: 0.125, weibullMinMultiplier: 0.6,
  weibullReinforceWeight: 0, weibullCrossSessionBoost: 0,
}

// editGroup takes a mounted form and exact desired group, drives DOM events and returns after React flushes.
// editGroup 接收已挂载表单及准确目标组，驱动 DOM 事件并在 React 刷新后返回。
async function editGroup(container: HTMLElement, policy: VmmNativeRetrievalPolicySummary): Promise<void> {
  // These are the component's three declared Boolean controls, not guessed alternate selectors.
  // 以下是组件声明的三个布尔控制项，不是猜测的候选选择器。
  const switches = { hybridEnabled: "向量与全文融合", mmrEnabled: "多样性选择（MMR）", weibullEnabled: "读取时衰减" }
  for (const [key, value] of Object.entries(policy)) {
    if (typeof value === "boolean") {
      // The typed Boolean branch follows the retrieval summary's explicit field types.
      // 此强类型布尔分支遵循检索摘要的显式字段类型。
      const label = Array.from(container.querySelectorAll("label")).find((entry) => entry.textContent === switches[key as keyof typeof switches])!
      expect(label).toBeDefined()
      // Each switch must actually change, preventing no-op edits from claiming coverage.
      // 每个开关必须实际变化，防止无操作编辑被计入覆盖。
      const button = label.querySelector<HTMLButtonElement>('[role="switch"]')!
      expect(button.getAttribute("aria-checked")).toBe(String(!value))
      await act(async () => button.click())
      expect(button.getAttribute("aria-checked")).toBe(String(value))
    } else {
      // Numeric fields and the exact decimal-string RRF control share native input events.
      // 数值字段及准确十进制字符串 RRF 控制项共用原生输入事件。
      const input = container.querySelector<HTMLInputElement>(`#vmm-ranking-${key}`)!
      expect(input).not.toBeNull()
      // The browser's native setter avoids reaching into component state.
      // 浏览器原生设置器避免访问组件内部状态。
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
      await act(async () => { setter.call(input, String(value)); input.dispatchEvent(new Event("input", { bubbles: true })) })
      expect(input.value).toBe(String(value))
    }
  }
}

// phase takes the owned directory and phase, waiting for Rust's independent persisted-state/business checks.
// phase 接收持有目录及阶段，等待 Rust 的独立持久状态及业务检查。
async function phase(directory: string, name: string): Promise<void> {
  await writeFile(join(directory, `${name}.ready`), "ready")
  // The lengthened scheduler phase intentionally observes more than one real minute.
  // 延长调度阶段明确需要观察超过真实一分钟。
  await vi.waitFor(() => access(join(directory, `${name}.ack`)), { interval: 50, timeout: 120000 })
}

// clickSave takes the mounted editor, clicks its actual enabled save button and flushes React events.
// clickSave 接收已挂载编辑器，点击其实际可用保存按钮并刷新 React 事件。
async function clickSave(container: HTMLElement): Promise<void> {
  // The label is shared by the global and embedded host retrieval editors.
  // 此文案由全局及内嵌宿主检索编辑器共享。
  const save = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "保存检索排序策略")!
  expect(save).toBeDefined()
  expect(save.disabled).toBe(false)
  await act(async () => save.click())
}

it.skipIf(!handshakeDirectory)("saves actual policy components through private HTTP and rebuilds persisted native state", async () => {
  if (!handshakeDirectory || !isAbsolute(handshakeDirectory)) throw new Error("Explicit absolute private handshake directory required")
  // Discovery is produced only by the companion isolated Rust controller; never print its capability.
  // 发现信息仅由配套隔离 Rust 控制器生成；绝不打印其中能力令牌。
  const discovery = JSON.parse(await readFile(join(handshakeDirectory, "discovery.json"), "utf8")) as { baseUrl: string; serviceToken: string }
  expect(typeof discovery.baseUrl).toBe("string")
  expect(typeof discovery.serviceToken).toBe("string")
  expect(discovery.serviceToken.length > 0).toBe(true)
  // Only a literal loopback HTTP origin advertised by this private fixture is authorized.
  // 仅授权本私有夹具公布的字面回环 HTTP 源。
  const endpoint = new URL(discovery.baseUrl)
  expect(endpoint.protocol).toBe("http:")
  expect(endpoint.hostname).toBe("127.0.0.1")
  expect(endpoint.port).not.toBe("")
  // Preserve the real transport, adding an outbound allowlist rather than fabricating responses.
  // 保留真实传输，增加出站白名单，不伪造响应。
  const realFetch = globalThis.fetch.bind(globalThis)
  // Record only method/path, never tokens or private payloads.
  // 仅记录方法及路径，不记录令牌或私有载荷。
  const requests: string[] = []
  // Results are observed after the actual HTTP facade resolves, never supplied by this test.
  // 结果仅在实际 HTTP 门面完成后被观察，绝不由本测试提供。
  const results: VmmMemoryActionResult[] = []
  // Retain the original DOM URL for deterministic cleanup.
  // 保留原始 DOM 地址以确定性清理。
  const originalUrl = window.location.href
  // This flag ensures frontend failure signals the bounded Rust waiter promptly.
  // 此标记确保前端失败时及时通知有界 Rust 等待器。
  let completed = false
  // The root owns only test DOM and is never mounted into a user's workbench.
  // 根节点仅持有测试 DOM，绝不挂载至用户工作台。
  const container = document.createElement("div")
  // One React root alternates between actual global and host editors.
  // 一个 React 根节点在实际全局及宿主编辑器间切换。
  const root = createRoot(container)
  document.body.append(container)
  try {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true)
    // Vitest exposes its owning jsdom instance; Node's existing storage globals otherwise take precedence.
    // Vitest 暴露其持有的 jsdom 实例；否则 Node 既有存储全局值会优先保留。
    const dom = (globalThis as unknown as { jsdom: { window: Window } }).jsdom
    vi.stubGlobal("localStorage", dom.window.localStorage)
    vi.stubGlobal("sessionStorage", dom.window.sessionStorage)
    vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
      // RequestInfo explicitly permits strings, URLs and Request objects; all resolve through this guard.
      // RequestInfo 明确允许字符串、URL 及 Request 对象；全部经过此守卫解析。
      const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, originalUrl)
      if (url.origin !== endpoint.origin || !["/health", "/rpc"].includes(url.pathname)) throw new Error("Private HTTP acceptance denied an out-of-scope request")
      requests.push(`${init?.method ?? "GET"} ${url.pathname}`)
      return realFetch(input, init)
    })
    window.localStorage.clear()
    window.sessionStorage.clear()
    clearBrowserServiceEndpointCache()
    window.history.replaceState(null, "", `/?${new URLSearchParams({ service: endpoint.origin, serviceToken: discovery.serviceToken })}`)
    expect(hasServiceRuntime()).toBe(true)
    // run forwards the component's declared optional payload through the production action facade.
    // run 将组件声明的可选载荷通过生产动作门面转发。
    // It returns the real response and records it only for independent phase assertions.
    // 返回真实响应，仅为独立阶段断言记录该响应。
    const run = async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult> => {
      // This exact request matches the existing global action bridge contract.
      // 此准确请求遵循既有全局动作桥契约。
      const result = await runVmmMemoryAction({ commandId: crypto.randomUUID(), action, scope: "global", projectId: null, payload: payload ?? {} })
      results.push(result)
      return result
    }
    // Initial state comes from native storage through HTTP, not frontend defaults or a response fixture.
    // 初始状态通过 HTTP 来自原生存储，不来自前端默认值或响应夹具。
    const initial = await run("native_runtime_policy")
    if (initial.data.kind !== "native_runtime_policy") throw new Error("Expected actual runtime policy response")
    // Retain the discriminated actual response across React's callback without inventing fallback state.
    // 跨 React 回调保留已判别的实际响应，不虚构回退状态。
    const initialPolicy = initial.data.value
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRetrievalPolicy settings={initialPolicy.retrieval} defaults={initialPolicy.retrievalDefaults} busy={false} onRunAction={run} onSaved={() => {}} /></I18nProvider>))
    await editGroup(container, globalPolicy)
    await clickSave(container)
    await act(async () => { await vi.waitFor(() => expect(results).toHaveLength(2), { timeout: 10000 }) })
    if (results[1].data.kind !== "native_runtime_policy") throw new Error("Expected saved runtime policy")
    expect(results[1].data.value.retrieval).toEqual(globalPolicy)
    expect(container.textContent).toContain("排序策略已保存")
    await phase(handshakeDirectory, "global")
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeHostProjectPolicy hostProjectId="replacement-host-a" busy={false} onRunAction={run} /></I18nProvider>))
    await act(async () => { await vi.waitFor(() => expect(results).toHaveLength(3), { timeout: 10000 }) })
    await editGroup(container, hostPolicy)
    await clickSave(container)
    await act(async () => { await vi.waitFor(() => expect(results).toHaveLength(4), { timeout: 10000 }) })
    if (results[3].data.kind !== "native_host_project_policy") throw new Error("Expected saved host policy")
    expect(results[3].data.value.overrides.retrieval).toEqual(hostPolicy)
    expect(results[3].data.value.effective.retrieval).toEqual(hostPolicy)
    await phase(handshakeDirectory, "host")
    // The component declares recall/retrieval/text inheritance controls in exactly this order.
    // 组件明确按召回、检索、文本的顺序声明继承控制项。
    const inherit = Array.from(container.querySelectorAll("button")).filter((button) => button.textContent === "改回跟随全局")
    expect(inherit).toHaveLength(3)
    expect(inherit[1].disabled).toBe(false)
    await act(async () => inherit[1].click())
    await act(async () => { await vi.waitFor(() => expect(results).toHaveLength(5), { timeout: 10000 }) })
    if (results[4].data.kind !== "native_host_project_policy") throw new Error("Expected restored host policy")
    expect(results[4].data.value.overrides.retrieval).toBeNull()
    expect(results[4].data.value.effective.retrieval).toEqual(globalPolicy)
    expect(container.querySelector<HTMLInputElement>("#vmm-ranking-rrfK")!.value).toBe(globalPolicy.rrfK)
    await phase(handshakeDirectory, "inherit")
    expect(requests).toEqual(["GET /health", ...Array<string>(5).fill("POST /rpc")])
    expect(results.map((result) => result.action)).toEqual(["native_runtime_policy", "native_retrieval_policy_update", "native_host_project_policy", "native_host_project_policy_update", "native_host_project_policy_update"])
    await verifyTextPhases(container, root, run, (name) => phase(handshakeDirectory, name))
    expect(requests).toEqual(["GET /health", ...Array<string>(15).fill("POST /rpc")])
    await verifyRecallPhases(container, root, run, (name) => phase(handshakeDirectory, name))
    expect(requests).toEqual(["GET /health", ...Array<string>(25).fill("POST /rpc")])
    await verifyProfilePhases(container, root, run, (name) => phase(handshakeDirectory, name))
    expect(requests).toEqual(["GET /health", ...Array<string>(34).fill("POST /rpc")])
    await verifyRetentionPhases(container, root, run, (name) => phase(handshakeDirectory, name))
    expect(requests).toEqual(["GET /health", ...Array<string>(55).fill("POST /rpc")])
    await phase(handshakeDirectory, "finish")
    completed = true
  } finally {
    if (!completed) await writeFile(join(handshakeDirectory, "abort.ready"), "frontend failed")
    await act(async () => root.unmount())
    container.remove()
    serviceClient.dispose()
    clearBrowserServiceEndpointCache()
    window.localStorage.clear()
    window.sessionStorage.clear()
    window.history.replaceState(null, "", originalUrl)
    vi.unstubAllGlobals()
  }
// Real scheduler acceptance crosses a full one-minute boundary in addition to the existing inference phases.
// 真实调度验收在既有推理阶段之外跨越完整一分钟边界。
}, 360000)
