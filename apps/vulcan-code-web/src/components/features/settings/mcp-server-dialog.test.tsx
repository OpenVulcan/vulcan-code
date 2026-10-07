// @vitest-environment jsdom
import { act, type ReactNode } from "react"
import { createRoot } from "react-dom/client"
import { describe, expect, it, vi } from "vitest"
import type { McpServerSummary } from "@/types"
import { McpServerDialog, type McpServerSaveRequest } from "./mcp-server-dialog"
import { createMcpDraft } from "./mcp-form-model"
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>()
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("zh-CN", key)
  return { ...actual, useI18n: () => ({ locale: "zh-CN", setLocale: () => undefined, t }) }
})

vi.mock("@/components/ui/dialog", () => ({
  Dialog: (props: { open: boolean; children: ReactNode }) => props.open ? <div>{props.children}</div> : null,
  DialogContent: (props: { children: ReactNode }) => <div>{props.children}</div>,
  DialogDescription: (props: { children: ReactNode }) => <p>{props.children}</p>,
  DialogFooter: (props: { children: ReactNode }) => <footer>{props.children}</footer>,
  DialogHeader: (props: { children: ReactNode }) => <header>{props.children}</header>,
  DialogTitle: (props: { children: ReactNode }) => <h2>{props.children}</h2>,
}))

describe("MCP save draft lifecycle", () => {
  it("keeps the failed draft, prevents duplicate saves, and resets on a new open lifetime", async () => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
    // pending is a service failure controlled entirely by this test.
    // pending 是完全由本测试控制的服务失败。
    let rejectSave: (error: Error) => void = () => undefined
    const pending = new Promise<void>((_, reject) => { rejectSave = reject })
    const save = vi.fn((_request: McpServerSaveRequest) => pending)
    const onOpenChange = vi.fn()
    const container = document.createElement("div")
    const root = createRoot(container)
    const server = { ...createMcpDraft(null), id: "fixture", name: "Retain draft", command: "node", args: [" a b ", ""] } as McpServerSummary
    document.body.append(container)
    // render keeps the same component instance while changing its open lifetime.
    // render 在改变打开生命周期时保持同一个组件实例。
    const render = async (open: boolean, editing: McpServerSummary | null) => act(async () => root.render(<McpServerDialog open={open} server={editing} onOpenChange={onOpenChange} actions={{ onSaveMcpServer: save, onRemoveMcpServer: vi.fn() }} />))
    try {
      await render(true, server)
      await act(async () => {
        const button = Array.from(container.querySelectorAll("button")).find(value => value.textContent === "保存")
        button?.click(); button?.click()
      })
      expect(save).toHaveBeenCalledOnce()
      expect(save.mock.calls[0]?.[0]).toMatchObject({ args: [" a b ", ""] })
      await act(async () => rejectSave(new Error("Synthetic save failure")))
      expect(onOpenChange).not.toHaveBeenCalled()
      expect(container.querySelector('[role="alert"]')?.textContent).toContain("Synthetic save failure")
      expect(Array.from(container.querySelectorAll("input")).some(input => input.value === "Retain draft")).toBe(true)
      await render(false, null)
      await render(true, null)
      expect(Array.from(container.querySelectorAll("input")).some(input => input.value === "Retain draft")).toBe(false)
      expect(container.querySelector('[role="alert"]')).toBeNull()
    } finally { await act(async () => root.unmount()); container.remove() }
  })

  it("sends OAuth fields only while OAuth sign-in is switched on", async () => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
    // save resolves immediately; only the submitted request matters here.
    // save 立即完成；这里只关心提交的请求。
    const save = vi.fn(async (_request: McpServerSaveRequest) => undefined)
    const container = document.createElement("div")
    const root = createRoot(container)
    document.body.append(container)
    try {
      for (const oauthEnabled of [true, false]) {
        const server = { ...createMcpDraft(null), id: "remote", name: "Remote", transport: "http", url: "https://mcp.example.test/mcp",
          oauthEnabled, oauthClientId: "client", oauthScopes: ["read"], oauthRedirectUri: "https://app.example.test/cb" } as McpServerSummary
        await act(async () => root.render(<McpServerDialog key={String(oauthEnabled)} open server={server} onOpenChange={vi.fn()} actions={{ onSaveMcpServer: save, onRemoveMcpServer: vi.fn() }} />))
        await act(async () => { Array.from(container.querySelectorAll("button")).find(value => value.textContent === "保存")?.click() })
      }
      expect(save.mock.calls[0]?.[0]).toMatchObject({ oauthEnabled: true, oauthClientId: "client", oauthScopes: ["read"], oauthRedirectUri: "https://app.example.test/cb" })
      expect(save.mock.calls[1]?.[0]).toMatchObject({ oauthEnabled: false, oauthClientId: "", oauthClientSecret: "", oauthScopes: [], oauthRedirectUri: "" })
    } finally { await act(async () => root.unmount()); container.remove() }
  })
})
