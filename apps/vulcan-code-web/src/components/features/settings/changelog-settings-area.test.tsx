// @vitest-environment jsdom
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { ChangelogSettingsArea } from "./changelog-settings-area"

// service replaces the bundled notes with three synthetic releases, newest first.
// service 以三个合成发布（从新到旧）替换打包的说明。
const service = vi.hoisted(() => ({
  releases: [{ version: "0.3.0", date: null }, { version: "0.2.0", date: "2026-11-02" }, { version: "0.1.0", date: "2026-10-26" }],
  load: vi.fn((version: string) => Promise.resolve(`notes-${version}`)),
}))
vi.mock("@/service/changelog", () => ({ CHANGELOG_RELEASES: service.releases, loadReleaseNotes: service.load }))

// JSDOM lacks the scroll plumbing used when a page turns.
// JSDOM 缺少翻页时使用的滚动能力。
const scrollDescriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollIntoView")
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() })
})
afterAll(() => {
  if (scrollDescriptor) Object.defineProperty(HTMLElement.prototype, "scrollIntoView", scrollDescriptor)
  else Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView")
})

let root: Root | null = null
let host: HTMLDivElement | null = null
afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  root = null
  host = null
  service.load.mockClear()
})

// render mounts the area for `appVersion` and waits for the first notes.
// render 为 `appVersion` 挂载区域并等待首份说明。
async function render(appVersion: string): Promise<HTMLDivElement> {
  host = document.createElement("div")
  document.body.append(host)
  root = createRoot(host)
  await act(async () => {
    root?.render(<I18nProvider initialLocale="en-US"><ChangelogSettingsArea appVersion={appVersion} /></I18nProvider>)
  })
  return host
}

// button finds the first pager button labelled `label`.
// button 查找第一个标签为 `label` 的翻页按钮。
function button(container: HTMLElement, label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((candidate) => candidate.textContent === label)
  if (!found) throw new Error(`missing ${label} button`)
  return found
}

describe("ChangelogSettingsArea", () => {
  it("opens on the running version and pages by release in the interface language", async () => {
    const container = await render("0.2.0")
    expect(service.load).toHaveBeenLastCalledWith("0.2.0", "en-US")
    expect(container.querySelector(".settings-changelog-head")?.textContent).toContain("Current version")
    expect(container.querySelector(".settings-changelog-position")?.textContent).toBe("2 / 3")

    await act(async () => button(container, "Older").click())
    expect(service.load).toHaveBeenLastCalledWith("0.1.0", "en-US")
    expect(button(container, "Older").disabled).toBe(true)
    expect(container.querySelector(".settings-changelog-head")?.textContent).not.toContain("Current version")

    await act(async () => button(container, "Newer").click())
    await act(async () => button(container, "Newer").click())
    expect(service.load).toHaveBeenLastCalledWith("0.3.0", "en-US")
    expect(button(container, "Newer").disabled).toBe(true)
    expect(container.querySelector(".settings-changelog-title")?.textContent).toContain("Unreleased")
  })

  it("starts at the newest release when the running version is not listed", async () => {
    const container = await render("9.9.9")
    expect(service.load).toHaveBeenLastCalledWith("0.3.0", "en-US")
    expect(container.querySelector(".settings-changelog-position")?.textContent).toBe("1 / 3")
  })
})
