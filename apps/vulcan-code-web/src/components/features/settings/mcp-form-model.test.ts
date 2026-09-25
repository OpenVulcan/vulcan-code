import { describe, expect, it } from "vitest"
import { createMcpDraft, parseMcpBulkPairs, validateMcpDraft, validateMcpPairs } from "./mcp-form-model"
import { readLocaleMessage, type Translate } from "@/i18n"

// zh resolves real Chinese messages so assertions read the shipped copy.
// zh 解析真实中文消息，使断言读取实际发布的文案。
const zh: Translate = key => readLocaleMessage("zh-CN", key)

describe("MCP exact configuration editing", () => {
  it("preserves argument boundaries, whitespace and empty arguments", () => {
    // draft is synthetic and never passed to a real service.
    // draft 是合成数据，不会传给真实服务。
    const draft = createMcpDraft(null)
    Object.assign(draft, { name: "fixture", command: "node", args: [" a b ", "", "--key=a=b"] })
    expect(validateMcpDraft(draft, zh)).toBeNull()
    expect(draft.args).toEqual([" a b ", "", "--key=a=b"])
  })
  it("splits bulk values at the first equals sign and validates malformed or duplicate rows", () => {
    expect(parseMcpBulkPairs("TOKEN=a=b=c\nOTHER= value ")).toEqual([{ key: "TOKEN", value: "a=b=c" }, { key: "OTHER", value: " value " }])
    expect(validateMcpPairs(parseMcpBulkPairs("MISSING"), "Env", false, zh)).toContain("第 1 行")
    expect(validateMcpPairs(parseMcpBulkPairs("Authorization=a\nauthorization=b"), "Headers", true, zh)).toContain("重复")
    expect(validateMcpPairs([{ key: "X-Test", value: "a\r\nInjected: b" }], "Headers", true, zh)).toContain("控制字符")
  })
  it("validates transport branches and host passthrough names independently", () => {
    const draft = createMcpDraft(null)
    Object.assign(draft, { name: "remote", transport: "http", url: "https://example.test/mcp", command: "", env: [{ key: "", value: "" }] })
    expect(validateMcpDraft(draft, zh)).toBeNull()
    draft.url = "file:///tmp/server"
    expect(validateMcpDraft(draft, zh)).toContain("HTTP(S)")
    Object.assign(draft, { transport: "stdio", command: "node", env: [], envPassthrough: ["TOKEN", "TOKEN"] })
    expect(validateMcpDraft(draft, zh)).toContain("重复")
  })
})
