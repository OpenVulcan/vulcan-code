import { describe, expect, it } from "vitest"
import { parseAnswerComponent } from "@/components/conversation/answer-components/answer-component-model"
import { SUPPORTED_LOCALES } from "@/i18n-locales"
import { CHANGELOG_RELEASES, loadReleaseNotes } from "./changelog"

// CONTAINERS hold Markdown that may nest further component fences.
// CONTAINERS 承载可再嵌套组件围栏的 Markdown。
const CONTAINERS = new Set(["ui-callout", "ui-details", "ui-tabs", "ui-columns"])

// componentFences lists every closed `ui-*` fence in `markdown`, including fences nested in containers.
// componentFences 列出 `markdown` 中全部已闭合的 `ui-*` 围栏，含嵌套在容器内的围栏。
// Parameters: markdown is release notes; returns name, info-string rest and body of each fence, or an unclosed marker.
// 参数：markdown 为更新说明；返回每个围栏的名称、信息串其余部分与正文，或未闭合标记。
function componentFences(markdown: string): Array<{ name: string; meta: string; body: string } | "unclosed"> {
  const lines = markdown.split("\n")
  const found: Array<{ name: string; meta: string; body: string } | "unclosed"> = []
  for (let index = 0; index < lines.length; index += 1) {
    const open = /^(`{3,})(ui-[a-z]+)?(.*)$/u.exec(lines[index])
    if (!open) continue
    const close = lines.findIndex((line, at) => at > index && new RegExp(`^\`{${open[1].length},}\\s*$`, "u").test(line))
    if (close < 0) {
      if (open[2]) found.push("unclosed")
      break
    }
    if (open[2]) {
      const body = lines.slice(index + 1, close).join("\n")
      found.push({ name: open[2], meta: open[3], body })
      if (CONTAINERS.has(open[2])) found.push(...componentFences(body))
    }
    index = close
  }
  return found
}

describe("release notes", () => {
  // Every registered language of every release must render through the conversation component parser.
  // 每个发布的每种注册语言都必须能经会话组件解析器渲染。
  it("ship valid answer components for every release and language", async () => {
    for (const release of CHANGELOG_RELEASES) {
      for (const { code } of SUPPORTED_LOCALES) {
        const notes = await loadReleaseNotes(release.version, code)
        expect(notes.trim(), `${release.version}/${code}`).not.toBe("")
        for (const fence of componentFences(notes)) {
          expect(fence, `${release.version}/${code} has an unclosed component fence`).not.toBe("unclosed")
          if (fence === "unclosed") continue
          // `ui-code` quotes files captured when a reply is sealed; release notes have no such capture.
          // `ui-code` 引用回答封口时抓取的文件；更新说明没有这一抓取。
          expect(fence.name, `${release.version}/${code}`).not.toBe("ui-code")
          expect(parseAnswerComponent(fence.name, fence.meta, fence.body), `${release.version}/${code}: invalid ${fence.name}${fence.meta}`).not.toBeNull()
        }
      }
    }
  })

  it("rejects a language that was not bundled", async () => {
    await expect(loadReleaseNotes(CHANGELOG_RELEASES[0].version, "xx-XX")).rejects.toThrow()
  })
})
