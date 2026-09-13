// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { LicenseBody, LicenseCatalog } from "@/service/licenses-service"
import { AboutLicensesSettingsArea, collectLicenseDependencies } from "./about-licenses-settings-area"

// service replaces only the transport boundary with isolated catalog/body promises.
// service 仅用隔离的目录与正文 Promise 替换传输边界。
const service = vi.hoisted(() => ({ catalog: vi.fn(), body: vi.fn() }))
vi.mock("@/service/licenses-service", () => ({ loadLicenseCatalog: service.catalog, loadLicenseBody: service.body }))

// fixture includes two versions and distinct source/body associations under one declared type.
// fixture 在一个声明类型下包含两个版本和不同来源及正文关联。
const fixture: LicenseCatalog = {
  schema_version: 2, scope: "rust-desktop-dependencies", source_file: "THIRD_PARTY_NOTICES.md", source_sha256: "revision",
  licenses: [{ id: "mit", title: "MIT License", status: "declared", sections: [
    { id: "a", start_line: 1, end_line: 2, dependencies: [{ name: "alpha", version: "1.0", source: "https://alpha.invalid" }] },
    { id: "b", start_line: 3, end_line: 4, dependencies: [{ name: "alpha", version: "2.0", source: "https://beta.invalid" }] },
  ] }],
}

// mount renders the actual shared controls and returns a cleanup function.
// mount 渲染真实共享控件，并返回清理函数。
async function mount() {
  const host = document.createElement("div")
  document.body.append(host)
  const root = createRoot(host)
  await act(async () => { root.render(<I18nProvider initialLocale="en-US"><AboutLicensesSettingsArea appVersion="test-version" /></I18nProvider>) })
  return { host, async dispose() { await act(async () => root.unmount()); host.remove() } }
}

// button finds visible buttons by their exact text in the synthetic surface.
// button 按精确文本查找合成界面中的可见按钮。
function button(host: HTMLElement, label: string): HTMLButtonElement {
  const element = [...host.querySelectorAll("button")].find((entry) => entry.textContent?.trim() === label)
  if (!element) throw new Error(`Missing button: ${label}`)
  return element
}

beforeEach(() => {
  Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true)
  service.catalog.mockReset().mockResolvedValue(fixture)
  service.body.mockReset().mockImplementation(async (id: string) => ({ section_id: id, text: `original-${id}` }))
})
afterEach(() => { document.body.innerHTML = "" })

describe("license catalog browsing", () => {
  it("keeps exact versions and preserves multiple bodies when deduplicating one package tuple", () => {
    const group = structuredClone(fixture.licenses[0])
    group.sections[1].dependencies.push(group.sections[0].dependencies[0])
    const dependencies = collectLicenseDependencies([group])
    expect(dependencies).toHaveLength(2)
    expect(dependencies[0].sectionIds).toEqual(["a", "b"])
  })

  it("loads only one original body and changes it through the dependency association", async () => {
    const view = await mount()
    expect(service.body).toHaveBeenCalledTimes(1)
    expect(service.body).toHaveBeenCalledWith("a", "revision")
    expect(view.host.querySelector("pre")?.textContent).toBe("original-a")
    await act(async () => button(view.host, "Body 2").click())
    expect(service.body).toHaveBeenLastCalledWith("b", "revision")
    expect(view.host.querySelector("pre")?.textContent).toBe("original-b")
    await view.dispose()
  })

  it("filters by actual structured source without collapsing versions", async () => {
    const view = await mount()
    expect(view.host.querySelectorAll("tbody tr")).toHaveLength(2)
    const input = view.host.querySelector("input")!
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "beta.invalid")
      input.dispatchEvent(new Event("input", { bubbles: true }))
    })
    expect(view.host.querySelectorAll("tbody tr")).toHaveLength(1)
    expect(view.host.querySelector("tbody")?.textContent).toContain("2.0")
    await view.dispose()
  })

  it("shows explicit artifact failure and retries the catalog read", async () => {
    service.catalog.mockRejectedValueOnce(new Error("missing generated index"))
    const view = await mount()
    expect(view.host.querySelector('[role="alert"]')?.textContent).toContain("missing generated index")
    await act(async () => button(view.host, "Reload").click())
    expect(view.host.querySelector('[role="alert"]')).toBeNull()
    expect(view.host.querySelector("pre")?.textContent).toBe("original-a")
    await view.dispose()
  })

  it("retains an explicitly unknown license title and its original body", async () => {
    const unknownCatalog = structuredClone(fixture)
    unknownCatalog.licenses[0].title = ""
    unknownCatalog.licenses[0].status = "unknown"
    service.catalog.mockResolvedValueOnce(unknownCatalog)
    const view = await mount()
    expect(view.host.querySelector("h3")?.textContent).toBe("Unknown license")
    expect(view.host.querySelector("pre")?.textContent).toBe("original-a")
    await view.dispose()
  })

  it("shows a failed body read and retries only that selected body", async () => {
    service.body.mockRejectedValueOnce(new Error("mismatched license revision"))
    const view = await mount()
    expect(view.host.querySelector('[role="alert"]')?.textContent).toContain("mismatched license revision")
    await act(async () => button(view.host, "Retry reading").click())
    expect(service.catalog).toHaveBeenCalledTimes(1)
    expect(view.host.querySelector("pre")?.textContent).toBe("original-a")
    await view.dispose()
  })

  it("bounds large dependency tables and preserves the next page", async () => {
    const largeCatalog = structuredClone(fixture)
    largeCatalog.licenses[0].sections[0].dependencies = Array.from({ length: 31 }, (_, index) => ({ name: `package-${index}`, version: "1.0", source: "https://source.invalid" }))
    largeCatalog.licenses[0].sections = [largeCatalog.licenses[0].sections[0]]
    service.catalog.mockResolvedValueOnce(largeCatalog)
    const view = await mount()
    expect(view.host.querySelectorAll("tbody tr")).toHaveLength(30)
    await act(async () => view.host.querySelector<HTMLButtonElement>('[aria-label="Next dependency page"]')!.click())
    expect(view.host.querySelectorAll("tbody tr")).toHaveLength(1)
    expect(view.host.querySelector("tbody")?.textContent).toContain("package-30")
    await view.dispose()
  })

  it("does not paint a late body response over a newer dependency selection", async () => {
    let resolveFirst!: (body: LicenseBody) => void
    service.body.mockImplementationOnce(() => new Promise<LicenseBody>((resolve) => { resolveFirst = resolve }))
    const view = await mount()
    await act(async () => button(view.host, "Body 2").click())
    expect(view.host.querySelector("pre")?.textContent).toBe("original-b")
    await act(async () => resolveFirst({ section_id: "a", text: "stale-body" }))
    expect(view.host.querySelector("pre")?.textContent).toBe("original-b")
    await view.dispose()
  })
})
