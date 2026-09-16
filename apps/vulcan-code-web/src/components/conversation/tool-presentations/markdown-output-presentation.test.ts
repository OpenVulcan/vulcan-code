import { describe, expect, it } from "vitest";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { resolveMarkdownToolPresentation } from "./markdown-output-presentation";

// markdownToolBlock builds one exact MCP invocation/result fixture for presentation mapping tests.
// markdownToolBlock 为展示映射测试构建一条精确的 MCP 调用/结果 fixture。
function markdownToolBlock(
  // Concrete public MCP tool name.
  // 具体的公开 MCP 工具名称。
  toolName: string,
  // Visible result body returned by the MCP adapter.
  // MCP 适配器返回的可见结果正文。
  body: string,
  // Exact invocation arguments stored in normalized metadata.
  // 规范化 metadata 中保存的精确调用参数。
  argumentsValue: Record<string, unknown>,
): ToolExecutionConversationBlock[] {
  return [{
    body,
    id: "markdown-tool-row",
    kind: "tool-result",
    rawMetadata: { arguments: argumentsValue },
    summary: "",
    title: toolName,
    toolCallId: "markdown-tool-call",
    toolName,
  }];
}

describe("recognized Markdown-output tool mapping", () => {
  it.each(["vulcan_file_list", "vulcan_file_read", "vulcan_codekit_rg", "vulcan_codekit_ast_tree"])(
    "shows %s results instead of the preceding invocation arguments",
    (toolName) => {
      // result mirrors the terminal block emitted after the request in the production V1 projection.
      // result 模拟生产 V1 投影中紧随请求生成的终态结果块。
      const result = markdownToolBlock(toolName, "# Returned content\nactual result", { path: "src" })[0]!;
      // request intentionally carries nonempty JSON, matching the reported directory-list card.
      // request 特意携带非空 JSON，与报告中的目录查询卡片一致。
      const request: ToolExecutionConversationBlock = {
        ...result, id: "request", kind: "tool-call", body: '{"path":"src","limit":50}',
      };
      expect(resolveMarkdownToolPresentation([request, result], "completed")?.markdown).toBe(result.body);
      expect(resolveMarkdownToolPresentation([request], "running")?.markdown).toBe("");
      expect(resolveMarkdownToolPresentation([request, { ...result, body: "" }], "completed")?.markdown).toBe("");
      expect(resolveMarkdownToolPresentation([request, { ...result, body: "permission denied" }], "failed")?.markdown).toBe("permission denied");
    },
  );

  it("keeps a file-read path and ranges in the collapsed model and parses source rows", () => {
    // presentation maps the documented vulcan-file-read request and text envelope.
    // presentation 映射已声明的 vulcan-file-read 请求与文本封套。
    const presentation = resolveMarkdownToolPresentation(
      markdownToolBlock(
        "vulcan_file_read",
        "[File:D:\\repo\\src\\main.ts Lines:80 Bytes:2048 Newline:LF Showing:L10-L12 Segments:1 Clipped:false]\nL10: const alpha = 1;\nL11: \nL12: export { alpha };",
        {
          file: "src/main.ts",
          segments: [{ start: 10, count: 3 }],
        },
      ),
      "completed",
    );

    expect(presentation).toMatchObject({
      contentKind: "source",
      summaryTargets: [{
        path: "src/main.ts",
        lineRanges: [{ startLine: 10, endLine: 12 }],
        numbered: true,
      }],
      sourceSections: [{
        path: "D:\\repo\\src\\main.ts",
        diffFile: {
          hunks: [{
            lines: [
              { content: "const alpha = 1;", kind: "context", oldLine: 10, newLine: 10 },
              { content: "", kind: "context", oldLine: 11, newLine: 11 },
              { content: "export { alpha };", kind: "context", oldLine: 12, newLine: 12 },
            ],
          }],
        },
      }],
      titleKey: "toolPresentation.fileRead.title",
    });
  });

  it("infers gutter line numbers for declared unnumbered multi-segment output", () => {
    // presentation uses the exact returned Showing ranges when numbered output is disabled.
    // presentation 在关闭行号输出时使用精确返回的 Showing 范围。
    const presentation = resolveMarkdownToolPresentation(
      markdownToolBlock(
        "vulcan_file_read",
        "[File:D:\\repo\\src\\main.ts Lines:20 Bytes:512 Newline:LF Showing:L2-L3,L7-L8 Segments:2 Clipped:false]\nalpha\nbeta\n\n--- L7+2 Showing:L7-L8 Clipped:false ---\ngamma\ndelta",
        {
          file: "src/main.ts",
          numbered: false,
          segments: [
            { start: 2, count: 2 },
            { start: 7, count: 2 },
          ],
        },
      ),
      "completed",
    );

    expect(presentation?.summaryTargets).toEqual([{
      lineRanges: [
        { startLine: 2, endLine: 3 },
        { startLine: 7, endLine: 8 },
      ],
      numbered: false,
      path: "src/main.ts",
    }]);
    expect(presentation?.sourceSections[0]?.diffFile.hunks[0]?.lines).toEqual([
      { content: "alpha", kind: "context", oldLine: 2, newLine: 2 },
      { content: "beta", kind: "context", oldLine: 3, newLine: 3 },
      { content: "gamma", kind: "context", oldLine: 7, newLine: 7 },
      { content: "delta", kind: "context", oldLine: 8, newLine: 8 },
    ]);
  });

  it("aligns batch source parsing after an earlier item fails", () => {
    // presentation uses the authoritative File i/n marker instead of counting only successful headers.
    // presentation 使用权威的 File i/n 标记，而不是仅统计成功文件头。
    const presentation = resolveMarkdownToolPresentation(
      markdownToolBlock(
        "vulcan_file_read",
        "[BatchFileRead Files:2 Limit:10]\n\n--- File 1/2 ---\n# FILE READ ERROR\n\n--- File 2/2 ---\n[File:D:\\repo\\src\\ok.ts Lines:10 Bytes:128 Newline:LF Showing:L5-L5 Segments:1 Clipped:false]\nL5: const ok = true;",
        {
          files: [
            { file: "src/missing.ts", numbered: false },
            { file: "src/ok.ts", numbered: true, segments: [{ start: 5, count: 1 }] },
          ],
        },
      ),
      "completed",
    );

    expect(presentation?.sourceSections[0]?.diffFile.hunks[0]?.lines).toEqual([
      { content: "const ok = true;", kind: "context", oldLine: 5, newLine: 5 },
    ]);
  });

  it("keeps protocol-shaped text as source inside an unnumbered declared range", () => {
    // presentation trusts the declared row count before interpreting later protocol separators.
    // presentation 在解释后续协议分隔符前信任已声明的返回行数。
    const presentation = resolveMarkdownToolPresentation(
      markdownToolBlock(
        "vulcan_file_read",
        "[File:D:\\repo\\src\\literal.txt Lines:1 Bytes:64 Newline:LF Showing:L1-L1 Segments:1 Clipped:false]\n--- File 2/2 ---",
        {
          file: "src/literal.txt",
          numbered: false,
          segments: [{ start: 1, count: 1 }],
        },
      ),
      "completed",
    );

    expect(presentation?.sourceSections[0]?.diffFile.hunks[0]?.lines).toEqual([
      { content: "--- File 2/2 ---", kind: "context", oldLine: 1, newLine: 1 },
    ]);
  });

  it("normalizes the runtime-supported legacy lines_rule separators and spacing", () => {
    // presentation follows the Lua runtime parser for comma whitespace and CR line separators.
    // presentation 遵循 Lua 运行时解析器对逗号空格与 CR 行分隔符的处理。
    const presentation = resolveMarkdownToolPresentation(
      markdownToolBlock(
        "vulcan_file_read",
        "",
        {
          file: "src/legacy.ts",
          lines_rule: "5, 2\r10 ,1",
        },
      ),
      "running",
    );

    expect(presentation?.summaryTargets).toEqual([{
      lineRanges: [
        { startLine: 5, endLine: 6 },
        { startLine: 10, endLine: 10 },
      ],
      numbered: true,
      path: "src/legacy.ts",
    }]);
  });

  it("keeps the queried file-list directory in the collapsed model", () => {
    // presentation maps the exact file-list path instead of deriving it from result prose.
    // presentation 映射精确文件列表路径，而不是从结果文本推导。
    const presentation = resolveMarkdownToolPresentation(
      markdownToolBlock(
        "vulcan_file_list",
        "[FileList:D:\\repo\\src Files:2]",
        { PWD: "D:\\repo", path: "src" },
      ),
      "completed",
    );

    expect(presentation).toMatchObject({
      contentKind: "markdown",
      summaryTargets: [{ path: "src", lineRanges: [], numbered: null }],
      titleKey: "toolPresentation.fileList.title",
    });
  });

  it("recognizes the Markdown menu with a localized title key and exact paths", () => {
    // presentation resolves during the request phase so the generic raw card never flashes first.
    // presentation 在请求阶段即完成解析，因此不会先闪现通用原始卡片。
    const presentation = resolveMarkdownToolPresentation(
      markdownToolBlock(
        "vulcan_codekit_markdown_menu",
        "# MARKDOWN MENU",
        { path: "D:\\repo\\docs\nD:\\repo\\README.md", recursive: false },
      ),
      "running",
    );

    expect(presentation).toMatchObject({
      contentKind: "markdown",
      kind: "markdown_tool_output",
      state: "running",
      summaryTargets: [
        { path: "D:\\repo\\docs", lineRanges: [], numbered: null },
        { path: "D:\\repo\\README.md", lineRanges: [], numbered: null },
      ],
      titleKey: "toolPresentation.codekit.markdownMenu.title",
      toolId: "vulcan_codekit_markdown_menu",
    });
  });
});
