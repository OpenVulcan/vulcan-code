// @vitest-environment jsdom
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { McpOAuthFlow, McpOAuthStatus } from "@/service/mcp-settings-service"
import { McpOAuthPanel } from "./mcp-oauth-panel"

// service mocks every OAuth RPC so no browser opens and no authorization server is contacted.
// service 模拟全部 OAuth RPC，不打开浏览器，也不联系授权服务器。
const { service, external } = vi.hoisted(() => ({
  service: {
    loadMcpOAuthStatus: vi.fn(), startMcpOAuth: vi.fn(), finishMcpOAuth: vi.fn(), cancelMcpOAuth: vi.fn(), removeMcpOAuth: vi.fn(),
  },
  external: { openExternalUrl: vi.fn(async () => undefined) },
}))
vi.mock("@/service/mcp-settings-service", () => service)
vi.mock("@/service/project-actions-service", () => external)
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>()
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key)
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) }
})

// status builds one synthetic OAuth view for the fixture server.
// status 为夹具服务器构建一个合成 OAuth 视图。
function status(authStatus: McpOAuthStatus["authStatus"], flow: Partial<McpOAuthFlow> | null = null): McpOAuthStatus {
  return {
    serverId: "remote", authStatus, statusError: authStatus === null ? "mcp_server_not_running" : null,
    flow: flow && {
      flowId: "flow-1", phase: "awaiting_callback", callbackMode: "loopback", authorizationUrl: "https://auth.example.test/authorize",
      redirectUri: "https://app.example.test/callback", error: null, expiresAtUnixMs: 0, ...flow,
    },
  }
}

// host is the synthetic host state: every operation replaces it and status reads return it.
// host 是合成宿主状态：每个操作替换它，状态读取返回它。
let host: McpOAuthStatus

// answer makes one operation move the synthetic host to `next`.
// answer 使一个操作把合成宿主切换到 `next`。
function answer(operation: ReturnType<typeof vi.fn>, next: McpOAuthStatus): void {
  operation.mockImplementation(async () => { host = next; return next })
}

// container and root own a fresh isolated mount for every scenario.
// container 与 root 为每个场景持有新的隔离挂载。
let container: HTMLDivElement
let root: Root

beforeEach(() => {
  vi.clearAllMocks()
  service.loadMcpOAuthStatus.mockImplementation(async () => host)
  container = document.createElement("div")
  document.body.appendChild(container)
  root = createRoot(container)
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
})
afterEach(async () => {
  await act(async () => root.unmount())
  container.remove()
})

// render mounts the panel and settles its first status read.
// render 挂载面板并等待首次状态读取完成。
async function render(serverId = "remote"): Promise<void> {
  await act(async () => { root.render(<McpOAuthPanel serverId={serverId} connectionStatus="awaiting_authorization" />) })
}

// button finds one rendered control by its exact label.
// button 按精确标签查找一个已渲染控件。
function button(label: string): HTMLButtonElement | undefined {
  return Array.from(container.querySelectorAll("button")).find(candidate => candidate.textContent === label)
}

// click activates one control and settles the resulting operation.
// click 激活一个控件并等待其操作完成。
async function click(label: string): Promise<void> {
  await act(async () => { button(label)?.click() })
}

describe("McpOAuthPanel", () => {
  it("completes a manual flow with the exact flow identity and pasted URL", async () => {
    host = status("not_authenticated")
    answer(service.startMcpOAuth, status("not_authenticated", { callbackMode: "manual" }))
    answer(service.finishMcpOAuth, status("authenticated", { phase: "succeeded", callbackMode: "manual" }))
    await render()

    await click("Sign in")
    expect(service.startMcpOAuth).toHaveBeenCalledWith("remote")
    expect(button("Finish sign-in")?.disabled).toBe(true)
    const input = container.querySelector<HTMLInputElement>("input")!
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "https://app.example.test/callback?code=c&state=s")
      input.dispatchEvent(new Event("input", { bubbles: true }))
    })
    await click("Finish sign-in")

    expect(service.finishMcpOAuth).toHaveBeenCalledWith({ serverId: "remote", flowId: "flow-1", callbackUrl: "https://app.example.test/callback?code=c&state=s" })
    expect(button("Sign out")).toBeDefined()
  })

  it("lets loopback flows finish by themselves and cancels the exact flow", async () => {
    host = status("not_authenticated", {})
    answer(service.cancelMcpOAuth, status("not_authenticated"))
    await render()

    expect(container.querySelector("input")).toBeNull()
    await click("Open authorization page")
    expect(external.openExternalUrl).toHaveBeenCalledWith("https://auth.example.test/authorize", true)
    await click("Cancel sign-in")
    expect(service.cancelMcpOAuth).toHaveBeenCalledWith({ serverId: "remote", flowId: "flow-1" })
    expect(button("Sign in")).toBeDefined()
  })

  it("offers sign-out when credentials exist or their state is unknown", async () => {
    host = status(null)
    answer(service.removeMcpOAuth, status("not_authenticated"))
    await render()

    expect(container.textContent).toContain("mcp_server_not_running")
    await click("Sign out")
    expect(service.removeMcpOAuth).toHaveBeenCalledWith("remote")
    expect(button("Sign out")).toBeUndefined()
  })

  it("keeps another server usable while one server's operation is still pending", async () => {
    host = status("not_authenticated")
    service.startMcpOAuth.mockImplementation(() => new Promise(() => undefined))
    await render()

    await click("Sign in")
    expect(button("Sign in")?.disabled).toBe(true)
    await render("other")

    expect(button("Sign in")?.disabled).toBe(false)
  })

  it("shows no sign-in controls for a server without OAuth", async () => {
    host = status("not_configured")
    await render()

    expect(button("Sign in")).toBeUndefined()
    expect(button("Sign out")).toBeUndefined()
  })
})
