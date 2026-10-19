// @vitest-environment jsdom
import { act, type ReactNode } from "react"
import { createRoot } from "react-dom/client"
import { describe, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { RenameDialog } from "./project-action-dialogs"
import { ProjectIconDialog } from "./project-icon-dialog"
import { ProjectFolderPickerDialog } from "./folder-picker-dialog"

// catalog is an isolated import boundary with no external icon service requests.
// catalog 是隔离的导入边界，不请求外部图标服务。
const catalog = vi.hoisted(() => ({ load: vi.fn(async () => ({ PROJECT_ICON_OPTIONS: [] })) }))
vi.mock("./project-icon-catalog-loader", () => ({ loadProjectIconCatalog: catalog.load }))
vi.mock("./project-icons", () => ({ ProjectIcon: () => <svg /> }))
vi.mock("@/ui-kit/primitives/dialog", () => ({
  Dialog: (props: { open: boolean; children: ReactNode }) => props.open ? <div>{props.children}</div> : null,
  DialogContent: (props: { children: ReactNode }) => <div>{props.children}</div>,
  DialogDescription: (props: { children: ReactNode }) => <p>{props.children}</p>,
  DialogFooter: (props: { children: ReactNode }) => <footer>{props.children}</footer>,
  DialogHeader: (props: { children: ReactNode }) => <header>{props.children}</header>,
  DialogTitle: (props: { children: ReactNode }) => <h2>{props.children}</h2>,
}))

// withEditor mounts one synthetic editor and disposes all local DOM resources afterward.
// withEditor 挂载一个合成编辑器，并在结束后释放全部本地 DOM 资源。
async function withEditor(element: ReactNode, check: (container: HTMLElement, render: (value: ReactNode) => Promise<void>) => Promise<void>) {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  const render = async (value: ReactNode) => act(async () => root.render(<I18nProvider initialLocale="zh-CN">{value}</I18nProvider>))
  try { await render(element); await check(container, render) }
  finally { await act(async () => root.unmount()); container.remove() }
}

describe("project editor operation lifetimes", () => {
  it("retains a failed rename and admits only one simultaneous submit", async () => {
    // reject controls one synthetic in-flight persistence request.
    // reject 控制单个合成的进行中持久化请求。
    let reject: (error: Error) => void = () => undefined
    const save = vi.fn(() => new Promise<void>((_, rejectRequest) => { reject = rejectRequest }))
    await withEditor(<RenameDialog target={{ kind: "project", id: "p", name: "Draft name" }} onCancel={vi.fn()} onConfirm={save} />, async container => {
      await act(async () => { container.querySelector("form")?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); container.querySelector("form")?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })) })
      expect(save).toHaveBeenCalledOnce()
      await act(async () => reject(new Error("Rename failed")))
      expect(container.querySelector('[role="alert"]')?.textContent).toContain("Rename failed")
      expect(container.querySelector("input")?.value).toBe("Draft name")
    })
  })
  it("shows a retryable catalog failure and retains the icon draft after save failure", async () => {
    catalog.load.mockRejectedValue(new Error("Catalog unavailable"))
    const save = vi.fn(async () => { throw new Error("Icon save failed") })
    await withEditor(<ProjectIconDialog target={{ project: { id: "p", name: "Project", path: "D:/synthetic", iconPack: "lucide", iconKey: "Folder" } }} onCancel={vi.fn()} onConfirm={save} onReset={vi.fn(async () => undefined)} />, async container => {
      expect(container.textContent).toContain("Catalog unavailable")
      catalog.load.mockResolvedValue({ PROJECT_ICON_OPTIONS: [] })
      await act(async () => Array.from(container.querySelectorAll("button")).find(button => button.textContent === "重试")?.click())
      expect(container.textContent).not.toContain("Catalog unavailable")
      await act(async () => container.querySelector<HTMLButtonElement>("footer button:last-child")?.click())
      expect(save).toHaveBeenCalledOnce()
      expect(container.textContent).toContain("Icon save failed")
      expect(container.querySelector("fieldset")).not.toBeNull()
    })
  })
  it("requires the edited path to be browsed before it can use directory selection authority", async () => {
    const browse = vi.fn(async () => ({ path: "D:/synthetic", canSelect: true, entries: [], roots: [] }))
    const confirm = vi.fn(async () => undefined)
    await withEditor(<ProjectFolderPickerDialog open onBrowse={browse} onConfirm={confirm} onOpenChange={vi.fn()} />, async container => {
      await act(async () => container.querySelector<HTMLButtonElement>("[data-folder-picker-edit]")!.click())
      // input changes bypass the React tracker just as native editing does.
      // input 变化像原生编辑一样绕过 React 值追踪器。
      const input = container.querySelector<HTMLInputElement>("form input")!
      await act(async () => {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "D:/unbrowsed")
        input.dispatchEvent(new Event("input", { bubbles: true }))
      })
      expect(container.querySelector<HTMLButtonElement>("[data-folder-picker-confirm]")?.disabled).toBe(true)
      expect(container.querySelector("[data-folder-picker-unbrowsed]")).not.toBeNull()
      expect(confirm).not.toHaveBeenCalled()
    })
  })
})
