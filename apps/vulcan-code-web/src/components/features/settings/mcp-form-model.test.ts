import { describe, expect, it } from "vitest"
import { createMcpDraft, parseMcpBulkPairs, validateMcpDraft, validateMcpPairs } from "./mcp-form-model"

describe("MCP exact configuration editing", () => {
  it("preserves argument boundaries, whitespace and empty arguments", () => {
    // draft is synthetic and never passed to a real service.
    // draft 是合成数据，不会传给真实服务。
    const draft = createMcpDraft(null)
    Object.assign(draft, { name: "fixture", command: "node", args: [" a b ", "", "--key=a=b"] })
    expect(validateMcpDraft(draft)).toBeNull()
    expect(draft.args).toEqual([" a b ", "", "--key=a=b"])
  })
  it("splits bulk values at the first equals sign and validates malformed or duplicate rows", () => {
    expect(parseMcpBulkPairs("TOKEN=a=b=c\nOTHER= value ")).toEqual([{ key: "TOKEN", value: "a=b=c" }, { key: "OTHER", value: " value " }])
    expect(validateMcpPairs(parseMcpBulkPairs("MISSING"), "Env")).toContain("第 1 行")
    expect(validateMcpPairs(parseMcpBulkPairs("Authorization=a\nauthorization=b"), "Headers", true)).toContain("重复")
    expect(validateMcpPairs([{ key: "X-Test", value: "a\r\nInjected: b" }], "Headers", true)).toContain("控制字符")
  })
  it("validates transport branches and host passthrough names independently", () => {
    const draft = createMcpDraft(null)
    Object.assign(draft, { name: "remote", transport: "http", url: "https://example.test/mcp", command: "", env: [{ key: "", value: "" }] })
    expect(validateMcpDraft(draft)).toBeNull()
    draft.url = "file:///tmp/server"
    expect(validateMcpDraft(draft)).toContain("HTTP(S)")
    Object.assign(draft, { transport: "stdio", command: "node", env: [], envPassthrough: ["TOKEN", "TOKEN"] })
    expect(validateMcpDraft(draft)).toContain("重复")
  })
})
