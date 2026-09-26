// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { describe, expect, it } from "vitest"
import { I18nProvider } from "@/i18n"
import { MarkdownMessage } from "./conversation-markdown"
import { splitStreamingMarkdownBlocks, STREAMING_MARKDOWN_RAW_TAIL_MAX_CHARS } from "./streaming-markdown-blocks"

describe("splitStreamingMarkdownBlocks", () => {
  // Paragraphs, headings, and tables separated by blank lines become independent blocks.
  // 由空行分隔的段落、标题与表格成为独立块。
  it("splits top-level blocks at blank lines", () => {
    expect(splitStreamingMarkdownBlocks("# Title\n\nFirst **para**\nsame para\n\n\n| a | b |\n| - | - |\n| 1 | 2 |\n"))
      .toEqual(["# Title", "First **para**\nsame para", "| a | b |\n| - | - |\n| 1 | 2 |"])
  })

  // Blank lines inside fenced code never split, including unclosed fences, nested fences, and CRLF input.
  // 围栏代码内部的空行从不拆分，包括未闭合围栏、嵌套围栏与 CRLF 输入。
  it("keeps fenced code whole", () => {
    expect(splitStreamingMarkdownBlocks("```ts\na\r\n\r\nb\r\n```\r\n\r\ntext")).toEqual(["```ts\na\n\nb\n```", "text"])
    expect(splitStreamingMarkdownBlocks("````md\n```\n\n```\n````\n\nafter")).toEqual(["````md\n```\n\n```\n````", "after"])
    expect(splitStreamingMarkdownBlocks("~~~\nstill\n\nopen")).toEqual(["~~~\nstill\n\nopen"])
    expect(splitStreamingMarkdownBlocks("```inline``` code\n\nnext")).toEqual(["```inline``` code", "next"])
  })

  // Loose lists, indented continuations, and multi-part quotes stay in one block with their blank lines.
  // 松散列表、缩进延续与多段引用连同空行保持在同一块中。
  it("keeps continuations with their block", () => {
    expect(splitStreamingMarkdownBlocks("- a\n\n- b\n\n  more b\n\n1. next list")).toEqual(["- a\n\n- b\n\n  more b\n\n1. next list"])
    expect(splitStreamingMarkdownBlocks("    code\n\n\n    more code\n\npara")).toEqual(["    code\n\n\n    more code", "para"])
    expect(splitStreamingMarkdownBlocks("> one\n\n> two\n\nafter")).toEqual(["> one\n\n> two", "after"])
    expect(splitStreamingMarkdownBlocks("para\n\n- list")).toEqual(["para", "- list"])
  })

  // Empty and blank-only bodies produce no blocks.
  // 空正文与仅含空行的正文不产生块。
  it("returns no blocks for blank input", () => {
    expect(splitStreamingMarkdownBlocks("")).toEqual([])
    expect(splitStreamingMarkdownBlocks("\n\n  \n")).toEqual([])
  })
})

// renderMarkdown returns the HTML one MarkdownMessage produces for a body in streaming or sealed mode.
// renderMarkdown 返回 MarkdownMessage 以流式或封存模式渲染正文得到的 HTML。
async function renderMarkdown(content: string, streaming: boolean): Promise<string> {
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="en-US"><MarkdownMessage content={content} eager streaming={streaming} /></I18nProvider>))
    return container.innerHTML
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
}

describe("streaming Markdown blocks", () => {
  // Rendering block by block must produce the same document as parsing the whole sealed body.
  // 逐块渲染必须产生与解析完整封存正文相同的文档。
  it("renders the same HTML as the sealed full-body parse", async () => {
    const body = [
      "## Findings",
      "Plain paragraph with `code` and **bold**.\nSecond line.",
      "- tight a\n- tight b",
      "1. loose one\n\n2. loose two\n\n   continued",
      "> quoted\n\n> still quoted",
      "| h1 | h2 |\n| --- | --- |\n| x | y |",
      "```rust\nfn main() {\n\n    println!(\"hi\");\n}\n```",
      "    indented code\n\n    after blank",
      "---",
      "Final ~~struck~~ paragraph.",
    ].join("\n\n")
    // react-markdown separates top-level elements with newline text nodes, which block splitting omits; they do not affect layout.
    // react-markdown 以换行文本节点分隔顶层元素，块拆分会省略这些节点；它们不影响布局。
    const withoutBlockSeparators = (html: string) => html.replace(/>\n</gu, "><")
    expect(withoutBlockSeparators(await renderMarkdown(body, true))).toBe(withoutBlockSeparators(await renderMarkdown(body, false)))
  })

  // An oversized unclosed tail renders as plain text instead of being re-parsed on every update.
  // 超长且未闭合的尾块以纯文本渲染，而不是在每次更新时重新解析。
  it("shows an oversized tail as plain text", async () => {
    const html = await renderMarkdown(`intro\n\n\`\`\`\n${"x".repeat(STREAMING_MARKDOWN_RAW_TAIL_MAX_CHARS)}`, true)
    expect(html).toContain("<p>intro</p>")
    expect(html).toContain('<pre class="streaming-markdown-raw-preview">')
  })
})
