import { beforeEach, describe, expect, it, vi } from "vitest"
import { loadMcpRuntimeSnapshot, loadMcpServerCapabilities } from "./mcp-settings-service"
import { saveMcpServerConfiguration, removeMcpServerConfiguration } from "./settings-actions-service"

// transport is an isolated RPC spy; no real service or remote server is contacted.
// transport 是隔离 RPC 观察器，不联系真实服务或远端服务器。
const transport = vi.hoisted(() => ({ available: true, request: vi.fn(async () => []) }))
vi.mock("@/service/webui-service-runtime", () => ({ hasServiceRuntime: () => transport.available, serviceClient: { request: transport.request } }))

describe("MCP metadata service boundary", () => {
  beforeEach(() => { transport.available = true; transport.request.mockClear() })
  it("uses only read-only catalog and optional metadata RPCs", async () => {
    await loadMcpRuntimeSnapshot()
    await loadMcpServerCapabilities("exact-server")
    expect(transport.request.mock.calls).toEqual([
      ["service_mcp_runtime_snapshot"], ["service_mcp_server_capabilities", { serverId: "exact-server" }],
    ])
  })
  it("rejects preview reads and writes without claiming successful persistence", async () => {
    transport.available = false
    await expect(loadMcpRuntimeSnapshot()).rejects.toThrow("unavailable")
    await expect(loadMcpServerCapabilities("exact-server")).rejects.toThrow("unavailable")
    await expect(saveMcpServerConfiguration({})).rejects.toThrow("not saved")
    await expect(removeMcpServerConfiguration("exact-server")).rejects.toThrow("not removed")
    expect(transport.request).not.toHaveBeenCalled()
  })
})
