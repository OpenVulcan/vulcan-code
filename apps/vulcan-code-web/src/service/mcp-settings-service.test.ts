import { beforeEach, describe, expect, it, vi } from "vitest"
import { cancelMcpOAuth, finishMcpOAuth, loadMcpOAuthStatus, loadMcpRuntimeSnapshot, loadMcpServerCapabilities, removeMcpOAuth, startMcpOAuth } from "./mcp-settings-service"
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
  it("sends OAuth operations to their host RPCs with the request envelope the host decodes", async () => {
    await loadMcpOAuthStatus("remote")
    await startMcpOAuth("remote")
    await finishMcpOAuth({ serverId: "remote", flowId: "flow-1", callbackUrl: "https://app.example.test/cb?code=c&state=s" })
    await cancelMcpOAuth({ serverId: "remote", flowId: "flow-1" })
    await removeMcpOAuth("remote")
    expect(transport.request.mock.calls).toEqual([
      ["service_mcp_oauth_status", { serverId: "remote" }],
      ["service_start_mcp_oauth", { serverId: "remote" }],
      ["service_finish_mcp_oauth", { request: { serverId: "remote", flowId: "flow-1", callbackUrl: "https://app.example.test/cb?code=c&state=s" } }],
      ["service_cancel_mcp_oauth", { request: { serverId: "remote", flowId: "flow-1" } }],
      ["service_remove_mcp_oauth", { serverId: "remote" }],
    ])
  })
  it("rejects preview reads and writes without claiming successful persistence", async () => {
    transport.available = false
    await expect(loadMcpRuntimeSnapshot()).rejects.toThrow("unavailable")
    await expect(loadMcpServerCapabilities("exact-server")).rejects.toThrow("unavailable")
    await expect(startMcpOAuth("exact-server")).rejects.toThrow("unavailable")
    await expect(saveMcpServerConfiguration({})).rejects.toThrow("not saved")
    await expect(removeMcpServerConfiguration("exact-server")).rejects.toThrow("not removed")
    expect(transport.request).not.toHaveBeenCalled()
  })
})
