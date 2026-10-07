// @vitest-environment jsdom
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { VulcanPluginPermissionPreview } from "@/types"
import { PluginPermissionChanges, addedAuthorization } from "./plugin-permission-changes"

// service mocks the installed-preview RPC so no plugin runtime is touched.
// service 模拟已安装预览 RPC，不接触任何插件运行时。
const { service } = vi.hoisted(() => ({ service: { loadInstalledPluginPermissionPreview: vi.fn() } }))
vi.mock("@/service/skills-actions-service", () => service)
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>()
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key)
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) }
})

// preview builds one synthetic host preview that each scenario overrides.
// preview 构建一份合成宿主预览，由各场景覆盖。
function preview(overrides: Partial<VulcanPluginPermissionPreview> = {}): VulcanPluginPermissionPreview {
  return {
    pluginId: "fixture", version: "1.0.0", contentFingerprint: "fp", origin: "user_installed",
    permissions: ["workspace_read"], hookPoints: [], uiMountPoints: [], binaryIds: [], nodeEntryIds: [], pythonEntryIds: [],
    installSecretRequirements: [], skillIds: [], agentIds: [], hasLuaRuntime: false,
    mcpServers: [{ serverId: "search", name: "Search", transport: "stdio", secretKeys: [] }],
    ...overrides,
  }
}

describe("addedAuthorization", () => {
  it("reports only access the candidate adds over the installed version", () => {
    const installed = preview()
    const candidate = preview({
      permissions: ["workspace_read", "network"], hasLuaRuntime: true,
      mcpServers: [{ serverId: "search", name: "Search", transport: "stdio", secretKeys: ["API_KEY"] }, { serverId: "web", name: "Web", transport: "http", secretKeys: [] }],
    })

    const changes = addedAuthorization(installed, candidate)

    expect(changes.permissions).toEqual(["network"])
    expect(changes.luaRuntime).toBe(true)
    expect(changes.mcpServers).toEqual(["Search (search, stdio)", "Web (web, http)"])
    expect(addedAuthorization(candidate, installed)).toMatchObject({ permissions: [], luaRuntime: false, mcpServers: [] })
  })
})

// container and root own a fresh isolated mount for every scenario.
// container 与 root 为每个场景持有新的隔离挂载。
let container: HTMLDivElement
let root: Root

beforeEach(() => {
  vi.clearAllMocks()
  container = document.createElement("div")
  document.body.appendChild(container)
  root = createRoot(container)
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
})
afterEach(async () => {
  await act(async () => root.unmount())
  container.remove()
})

describe("PluginPermissionChanges", () => {
  it("warns with the added entries of an escalating update", async () => {
    service.loadInstalledPluginPermissionPreview.mockResolvedValue(preview())
    await act(async () => root.render(<PluginPermissionChanges candidate={preview({ permissions: ["workspace_read", "network"] })} />))

    expect(service.loadInstalledPluginPermissionPreview).toHaveBeenCalledWith("fixture")
    expect(container.querySelector("[data-tone='warning'] dd")?.textContent).toBe("network")
  })

  it("confirms when an update adds nothing", async () => {
    service.loadInstalledPluginPermissionPreview.mockResolvedValue(preview())
    await act(async () => root.render(<PluginPermissionChanges candidate={preview()} />))

    expect(container.querySelector("[data-tone='info']")).not.toBeNull()
    expect(container.querySelector("dd")).toBeNull()
  })

  it("surfaces an unreadable installed version instead of claiming no changes", async () => {
    service.loadInstalledPluginPermissionPreview.mockRejectedValue(new Error("Vulcan plugin `fixture` is not installed."))
    await act(async () => root.render(<PluginPermissionChanges candidate={preview()} />))

    expect(container.querySelector("[data-tone='warning']")?.textContent).toContain("is not installed")
    expect(container.querySelector("[data-tone='info']")).toBeNull()
  })
})
