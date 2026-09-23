// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { describe, expect, it, vi } from "vitest"
import type { McpServerSummary, StartupSnapshot } from "@/types"
import { createMcpDraft } from "./mcp-form-model"
import { McpSettingsArea } from "./mcp-settings-area"

// service records read-only operations against synthetic server metadata.
// service 记录针对合成服务器元数据的只读操作。
const service = vi.hoisted(() => ({
  snapshot: vi.fn(async () => [{ inspectionGeneration: 1, state: {
    id: "fixture", name: "Fixture", source: "user", pluginId: null, generation: null, transport: "stdio", endpoint: "node",
    status: "connected", retryCount: 0, protocolVersion: "fixture-protocol", serverProduct: "fixture", serverVersion: "1",
    toolCount: 1, promptCount: 1, resourceCount: 0, lastError: null, recentLogs: ["this server only"],
  }, tools: [{ name: "catalog_tool", description: "Original tool", inputSchema: { type: "object", properties: { choice: { type: "string", enum: ["a", "b"] } }, required: ["choice"], "x-preserve": true }, outputSchema: { type: "string" } }] }]),
  capabilities: vi.fn(async () => ({ prompts: [{ name: "prompt" }], resources: [], promptError: null, resourceError: null })),
}))
vi.mock("@/service/mcp-settings-service", () => ({ loadMcpRuntimeSnapshot: service.snapshot, loadMcpServerCapabilities: service.capabilities }))
vi.mock("./mcp-server-dialog", () => ({ McpServerDialog: () => null }))
vi.mock("./mcp-text", () => ({ useMcpText: () => (zh: string) => zh }))

describe("MCP runtime inspection boundary", () => {
  it("reads optional metadata only after opening its tab and excludes global logs", async () => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
    service.capabilities.mockClear()
    // container, root and config are test-owned resources only.
    // container、root 与 config 仅为测试自有资源。
    const container = document.createElement("div")
    const root = createRoot(container)
    const config = { ...createMcpDraft(null), id: "fixture", name: "Fixture" } as McpServerSummary
    const diagnostics = vi.fn()
    document.body.append(container)
    // click selects a visible action by its exact display text.
    // click 按精确展示文案选择可见动作。
    const click = async (text: string) => act(async () => { Array.from(container.querySelectorAll("button")).find(button => button.textContent === text)?.click() })
    try {
      await act(async () => root.render(<McpSettingsArea servers={[config]} actions={{ onSaveMcpServer: vi.fn(), onRemoveMcpServer: vi.fn() }} logs={[{ id: "global", source: "mcp", message: "GLOBAL_SECRET_LOG", level: "info" } as never]} modules={[]} startup={{ steps: [] } as unknown as StartupSnapshot} onOpenDiagnostics={diagnostics} />))
      expect(container.textContent).toContain("已连接")
      expect(service.capabilities).not.toHaveBeenCalled()
      await click("工具")
      expect(container.querySelector(".mcp-schema")).toBeNull()
      await act(async () => {
        // trigger expands the tool row without invoking a tool action.
        // trigger 展开工具条目，不调用工具动作。
        container.querySelector<HTMLButtonElement>('.mcp-tool [data-slot="disclosure-trigger"]')?.click()
      })
      expect(container.textContent).toContain("x-preserve")
      expect(container.textContent).toContain("必填")
      expect(service.capabilities).not.toHaveBeenCalled()
      // A nonempty retained catalog still needs explicit feedback when the current query has no rows.
      // 保留目录非空但当前查询没有条目时，仍需明确反馈。
      const toolSearch = container.querySelector<HTMLInputElement>('[aria-label="搜索工具"]')!
      await act(async () => {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(toolSearch, "unmatched")
        toolSearch.dispatchEvent(new Event("input", { bubbles: true }))
      })
      expect(container.querySelector('[role="status"]')?.textContent).toBe("没有匹配的工具")
      expect(container.querySelectorAll(".mcp-tool")).toHaveLength(0)
      await click("提示词与资源")
      expect(service.capabilities).toHaveBeenCalledExactlyOnceWith("fixture")
      // A new catalog generation invalidates optional metadata even if the server stays connected under the same ID.
      // 即使服务器保持同标识且已连接，新目录代次也会使可选元数据失效。
      const previous = (await service.snapshot.mock.results[0].value)[0]
      service.snapshot.mockResolvedValueOnce([{ ...previous, inspectionGeneration: 2 }])
      await click("刷新状态")
      expect(service.capabilities).toHaveBeenCalledTimes(2)
      await click("诊断")
      expect(container.textContent).toContain("this server only")
      expect(container.textContent).not.toContain("GLOBAL_SECRET_LOG")
      await click("打开运行诊断")
      expect(diagnostics).toHaveBeenCalledOnce()
    } finally { await act(async () => root.unmount()); container.remove() }
  })
})
