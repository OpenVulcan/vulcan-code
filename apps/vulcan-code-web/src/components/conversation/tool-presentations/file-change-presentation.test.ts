import { describe, expect, it } from "vitest";
import type { PersistentChangeSetFile } from "@/service/conversation-event-window-store";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import {
  fileChangeStatusLabel,
  hydrateFileAccessPresentation,
  hydrateFileChangePresentation,
  resolveFileAccessPresentation,
  resolveFileChangePresentation,
} from "./file-change-presentation";

/// Builds one compact tool block fixture with the exact public tool identity and metadata bridge.
/// 构建带有精确公开工具标识与 metadata 桥接数据的紧凑工具块 fixture。
function toolBlock(
  toolName: string,
  body: string,
  rawMetadata: Record<string, unknown>,
): ToolExecutionConversationBlock[] {
  return [{
    body,
    id: "tool-row",
    kind: "tool-result",
    rawMetadata,
    summary: "",
    title: toolName,
    toolCallId: "tool-call",
    toolName,
  }];
}

describe("native file presentation mapping", () => {
  // Both delivery paths must show the shared Markdown without rebuilding legacy JSON rows.
  // 两种加载路径都必须展示共享 Markdown，不重新构造旧 JSON 行。
  it.each(["list", "stat"] as const)("renders inline and lazy %s Markdown", (action) => {
    // The event fixture follows the persisted tool-call envelope.
    // 事件夹具遵循持久化工具调用信封。
    const body = "# File information\n\nPath: `src/example.ts`\n";
    const metadata = {
      arguments: { action, path: "src" },
      v1_event: true,
      v1_tool_event: {
        kind: "tool_call", api_call_id: 1, tool_call_id: 1, tool_name: "file",
        input: { action, path: "src" }, request_stream_id: "inspection",
        status: "completed", output: { model_output: body, host_result: null },
      },
    };
    // Inline and deferred results must converge on the same readable body.
    // 即时与延迟结果必须得到相同可读正文。
    const inline = resolveFileAccessPresentation(toolBlock("file", body, metadata), "completed", {});
    const pending = resolveFileAccessPresentation(toolBlock("file", "", {
      arguments: { action, path: "src" },
    }), "completed", {});
    const hydrated = hydrateFileAccessPresentation(pending!, { body, metadata: {} });
    expect(inline).toMatchObject({ markdownBody: body, interactive: true });
    expect(hydrated).toMatchObject({ markdownBody: body, interactive: true });
  });

  it.each(["file_edit", "apply_patch"] as const)("keeps %s counts growing after preview truncation", (toolName) => {
    // Parser totals are authoritative even when the display payload has only one retained line.
    // 即使展示载荷仅保留一行，解析器总数仍是权威。
    const presentation = resolveFileChangePresentation(toolBlock(toolName, "", {}), "running", {
      requestPreview: { kind: toolName, revision: 9, affected_resources: ["large.txt"],
        progress_fields: { old_line_count: 3000, new_line_count: 6000 },
        structured_lines: [],
        body_preview: toolName === "apply_patch"
          ? "*** Update File: large.txt\n+preview" : "large.txt (replace)\n+ preview",
        redaction_flags: ["structured_preview_truncated", "body_preview_truncated"],
      },
    });
    expect(presentation).toMatchObject({ totalAdded: 6000, totalRemoved: 3000,
      files: [{ added: 6000, removed: 3000 }] });
  });
  // V1 no longer needs a duplicated host-result envelope to restore mutation rows and counts.
  // V1 不再需要重复的宿主结果信封即可恢复变更行与统计。
  it.each([
    [{ change: "create", path: "new.py", hunks: [], lifecycle_content: { mode: "full", content: "one\ntwo\n", total_line_count: 2 } }, 2, 0],
    [{ change: "modify", path: "edit.py", hunks: [{ before: "", after: "", deleted: [{ line: 1, content: "old" }], inserted: [{ line: 1, content: "new" }] }] }, 1, 1],
    [{ change: "delete", path: "gone.py", hunks: [], lifecycle_content: { mode: "truncated", head: "first", tail: "last", total_line_count: 200 } }, 0, 200],
  ] satisfies [PersistentChangeSetFile, number, number][])("restores canonical changeset %j", (file, added, removed) => {
    // blocks carry the same event metadata authored by the production branch projector.
    // blocks 携带生产分支投影器产出的相同事件元数据。
    const blocks = toolBlock("file_edit", "done", { v1_event: true, v1_tool_event: {
      kind: "tool_call", api_call_id: 1, tool_call_id: 1, tool_name: "file_edit", input: { path: file.path },
      request_stream_id: "canonical-edit", status: "completed", changeset: { mode: "applied", files: [file] },
    } });
    // presentation reads only retained data and never reconstructs changes from the input.
    // presentation 仅读取保留数据，不从输入重建变更。
    const presentation = resolveFileChangePresentation(blocks, "completed", {});
    expect(presentation).toMatchObject({ interactive: true, totalAdded: added, totalRemoved: removed });
    expect(presentation?.files[0]).toMatchObject({ expandable: true, operation: file.change });
    expect(presentation?.files[0].diffFile).not.toBeNull();
    if (file.change === "delete") expect(presentation?.files[0].rows).toContainEqual({ kind: "omit", lineNumber: null, content: "省略 198 行" });
  });

  it("reports execution and effective-change axes independently", () => {
    // stoppedPresentation has no host ChangeSet and therefore represents a truthful stopped, unedited result.
    // stoppedPresentation 没有宿主 ChangeSet，因此表示真实的已停止且未编辑结果。
    const stoppedPresentation = resolveFileChangePresentation(
      toolBlock("file_edit", "stopped", { arguments: { path: "src/a.ts" } }),
      "stopped",
      {},
    );
    // failedPresentation has no host ChangeSet and therefore represents a failed, unedited result.
    // failedPresentation 没有宿主 ChangeSet，因此表示失败且未编辑结果。
    const failedPresentation = resolveFileChangePresentation(
      toolBlock("file_edit", "failed", { arguments: { path: "src/a.ts" } }),
      "failed",
      {},
    );

    expect(fileChangeStatusLabel(stoppedPresentation!, "modify")).toBe("Stopped · Not edited");
    expect(fileChangeStatusLabel(failedPresentation!, "modify")).toBe("Failed · Not edited");
  });

  it.each(["vulcan.file_read.v1", "vulcan.file_read.v2"])("renders and hydrates %s using metadata coordinates", (schema) => {
    // Include a blank source line and a literal fence to prove headers are never reparsed as source.
    // 包含源文件空行与字面围栏，证明标题不会被重新解析为源文件正文。
    const sourceLines = ["alpha", "", "```", "你好", "L14: original prefix"];
    // v2 adds exactly one positional prefix; literal prefixes inside source must survive.
    // v2 仅增加一个位置前缀；源文件自身的字面前缀必须保留。
    const renderedLines = sourceLines.map((line, index) => schema === "vulcan.file_read.v2" ? `L${10 + index}: ${line}` : line);
    const body = "# File: src/a.ts\n\nShowing selected source\n\n````typescript\n" + renderedLines.join("\n") + "\n````\n";
    const firstSourceIndex = body.split("\n").indexOf(renderedLines[0]);
    const metadata = {
      arguments: { action: "read", path: "src/a.ts", start_line: 10, end_line: 13 },
      host_result: {
        kind: "file_access",
        payload: {
          schema,
          action: "read",
          path: "D:/repo/src/a.ts",
          range: { source_start_line: 10, output_start_line: firstSourceIndex + 1, line_count: sourceLines.length },
        },
      },
    };
    const inline = resolveFileAccessPresentation(toolBlock("file", body, metadata), "completed", {});
    const pending = resolveFileAccessPresentation(toolBlock("file", "", metadata), "completed", {});
    const hydrated = hydrateFileAccessPresentation(pending!, { body, metadata });
    const expectedRows = sourceLines.map((content, index) => ({ content, kind: "context", lineNumber: 10 + index }));
    expect(pending?.items[0].rows).toEqual([]);
    expect(inline?.items[0].rows).toEqual(expectedRows);
    expect(hydrated.items[0].rows).toEqual(expectedRows);
    expect(hydrated.items[0].lineRanges).toEqual([{ startLine: 10, endLine: 10 + sourceLines.length - 1 }]);
    expect(JSON.stringify(metadata)).not.toContain("alpha");
  });

  it("keeps empty Markdown reads empty and rejects invalid line coordinates", () => {
    // Invalid metadata must never fall back to guessed content fields or Markdown parsing.
    // 无效元数据不得退回猜测正文字段或解析 Markdown。
    const body = "# Empty file\n\n```text\n```\n";
    const firstSourceLine = body.split("\n").indexOf("```text") + 2;
    const metadata = {
      arguments: { action: "read", path: "empty.txt" },
      host_result: { kind: "file_access", payload: {
        schema: "vulcan.file_read.v1", action: "read",
        range: { source_start_line: 1, output_start_line: firstSourceLine, line_count: 0 },
      } },
    };
    const empty = resolveFileAccessPresentation(toolBlock("file", body, metadata), "completed", {});
    expect(empty?.items[0].rows).toEqual([]);
    expect(empty?.items[0].lineRanges).toEqual([]);
    // Lazy empty results replace the requested range even though no code rows are returned.
    // 延迟加载的空结果即使没有代码行，也会替换请求范围。
    const pending = resolveFileAccessPresentation(toolBlock("file", "", {
      arguments: { action: "read", path: "empty.txt", start_line: 100 },
    }), "completed", {});
    const hydrated = hydrateFileAccessPresentation(pending!, { body, metadata });
    expect(hydrated.items[0].lineRanges).toEqual([]);
    expect(hydrated.items[0].expandable).toBe(false);
    metadata.host_result.payload.range.line_count = body.split("\n").length + 1;
    const invalid = resolveFileAccessPresentation(toolBlock("file", body, metadata), "completed", {});
    expect(invalid?.items[0].rows).toEqual([
      { content: "Invalid file read line mapping.", kind: "error", lineNumber: null },
    ]);
  });

  it("renders a native v2 read range as code rows with actual gutter line numbers", () => {
    const presentation = resolveFileAccessPresentation(
      toolBlock("file", "", {
        arguments: {
          action: "read",
          path: "src/a.ts",
          start_line: 10,
          end_line: 12,
        },
        host_result: {
          kind: "file_access",
          payload: {
            schema: "vulcan.file_access.v2",
            action: "read",
            path: "D:/repo/src/a.ts",
            encoding: "UTF-8",
            bom: false,
            line_ending: "mixed",
            mixed_line_endings: true,
            final_newline: false,
            start_line: 10,
            end_line: 12,
            content: "alpha\r\nbeta\n\romega",
          },
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(presentation).toMatchObject({
      action: "read",
      kind: "file_read",
      items: [{
        lineRanges: [{ startLine: 10, endLine: 12 }],
        textFormat: {
          encoding: "UTF-8",
          bom: false,
          lineEnding: "mixed",
          mixedLineEndings: true,
          finalNewline: false,
        },
        rows: [
          { content: "alpha", kind: "context", lineNumber: 10 },
          { content: "beta", kind: "context", lineNumber: 11 },
          { content: "omega", kind: "context", lineNumber: 12 },
        ],
        diffFile: {
          hunks: [{
            lines: [
              { content: "alpha", kind: "context", oldLine: 10, newLine: 10 },
              { content: "beta", kind: "context", oldLine: 11, newLine: 11 },
              { content: "omega", kind: "context", oldLine: 12, newLine: 12 },
            ],
          }],
        },
      }],
    });
  });

  it("keeps historical multi-file read ranges structured and hydrates authoritative host rows", () => {
    const presentation = resolveFileAccessPresentation(
      toolBlock("file", "", {
        arguments: {
          action: "read",
          targets: [{
            path: "src/a.ts",
            segments: [
              { start_line: 10, line_count: 3 },
              { start_line: 30, line_count: 1 },
            ],
          }, {
            path: "src/b.ts",
            segments: [{ start_line: 40, line_count: 2 }],
          }],
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
      { sessionId: "session-1", recordId: "detail-1" },
    );

    expect(presentation).toMatchObject({
      detailRef: { sessionId: "session-1", recordId: "detail-1" },
      interactive: false,
      items: [
        {
          lineRanges: [
            { startLine: 10, endLine: 12 },
            { startLine: 30, endLine: 30 },
          ],
          path: "src/a.ts",
          rows: [],
        },
        {
          lineRanges: [{ startLine: 40, endLine: 41 }],
          path: "src/b.ts",
          rows: [],
        },
      ],
    });

    const hydrated = hydrateFileAccessPresentation(presentation!, {
      body: "Result: model-facing envelope must not replace native source rows",
      metadata: {
        host_result: {
          kind: "file_access",
          payload: {
            action: "read",
            results: [{
              ok: true,
              lines: [
                { line: 10, content: "alpha" },
                { line: 11, content: "beta" },
                { line: 30, content: "omega" },
              ],
            }, {
              ok: true,
              lines: [
                { line: 40, content: "delta" },
                { line: 41, content: "epsilon" },
              ],
            }],
          },
        },
      },
    });

    expect(hydrated).toMatchObject({
      interactive: true,
      items: [
        {
          diffFile: {
            hunks: [{
              lines: [
                { content: "alpha", kind: "context", oldLine: 10, newLine: 10 },
                { content: "beta", kind: "context", oldLine: 11, newLine: 11 },
                { content: "omega", kind: "context", oldLine: 30, newLine: 30 },
              ],
            }],
          },
          expandable: true,
          rows: [
            { lineNumber: 10, content: "alpha" },
            { lineNumber: 11, content: "beta" },
            { lineNumber: 30, content: "omega" },
          ],
        },
        {
          expandable: true,
          rows: [
            { lineNumber: 40, content: "delta" },
            { lineNumber: 41, content: "epsilon" },
          ],
        },
      ],
    });
  });

  it("does not let model-result detail rewrite authoritative change_set rows", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        arguments: {
          path: "D:/repo/src/main.ts",
          old_string: "const oldValue = 1;",
          new_string: "const newValue = 2;",
        },
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [{
              change: "modify",
              path: "D:/repo/src/main.ts",
              hunks: [{
                before: "",
                delete: [{ line: 70, content: "const oldValue = 1;" }],
                insert: [{ line: 70, content: "const newValue = 2;" }],
                after: "",
              }],
            }],
          },
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );

    const hydrated = hydrateFileChangePresentation(presentation!, {
      body: JSON.stringify({
        ok: true,
        operations: [{
          request_path: "D:/repo/src/main.ts",
          original_range: { start_line: 999, line_count: 20 },
          final_range: { start_line: 999, line_count: 30 },
        }],
      }),
      metadata: {
        arguments: {
          path: "D:/repo/src/main.ts",
          old_string: "fabricated old",
          new_string: "fabricated new",
        },
      },
    });

    expect(hydrated).toBe(presentation);
    expect(hydrated.files[0]?.rows).toEqual([
      { content: "const oldValue = 1;", kind: "delete", lineNumber: 70 },
      { content: "const newValue = 2;", kind: "insert", lineNumber: 70 },
    ]);
  });

  it("renders every authoritative persisted whole-file delete and insert row", () => {
    const deleted = Array.from({ length: 208 }, (_, index) => ({ line: index + 1, content: `old-${index + 1}` }));
    const inserted = Array.from({ length: 140 }, (_, index) => ({ line: index + 1, content: `new-${index + 1}` }));
    const presentation = resolveFileChangePresentation(
      toolBlock("file", "", {
        arguments: {
          action: "write",
          path: "D:/repo/README.md",
          content: inserted.map((line) => line.content).join("\n"),
        },
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [{
              resource_id: "resource-1",
              change: "modify",
              path: "D:/repo/README.md",
              hunks: [{ before: "", delete: deleted, insert: inserted, after: "" }],
            }],
          },
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(presentation).toMatchObject({
      interactive: true,
      source: "native_file_mutation",
      totalAdded: 140,
      totalRemoved: 208,
      files: [{
        added: 140,
        removed: 208,
        expandable: true,
      }],
    });
    expect(presentation?.files[0]?.rows).toHaveLength(348);
    expect(presentation?.files[0]?.rows.slice(0, 208)).toEqual(
      deleted.map((line) => ({ content: line.content, kind: "delete", lineNumber: line.line })),
    );
    expect(presentation?.files[0]?.rows.slice(208)).toEqual(
      inserted.map((line) => ({ content: line.content, kind: "insert", lineNumber: line.line })),
    );
  });

  it("does not synthesize missing terminal change_set hunks from request arguments", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        arguments: {
          path: "D:/repo/src/main.ts",
          old_string: "const oldValue = 1;",
          new_string: "const newValue = 2;\nreturn newValue;",
        },
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [{
              resource_id: "resource-1",
              change: "modify",
              path: "D:/repo/src/main.ts",
              inserted_line_count: 7,
              deleted_line_count: 5,
              hunks: [],
            }],
            extensions: {
              "vulcan.file_tools.v2": {
                resources: [{
                  resource_id: "resource-1",
                  after_text_format: {
                    encoding: "utf-8",
                    bom: false,
                    line_ending: "lf",
                  },
                }],
              },
            },
          },
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(presentation).toMatchObject({
      interactive: false,
      source: "native_file_edit",
      totalAdded: 7,
      totalRemoved: 5,
      files: [{
        added: 7,
        removed: 5,
        expandable: false,
        textFormat: {
          encoding: "utf-8",
          bom: false,
          lineEnding: "lf",
        },
        rows: [],
      }],
    });
  });

  it("does not present completed mutation requests as applied diffs when change_set is absent", () => {
    const fileWrite = resolveFileChangePresentation(
      toolBlock("file", "", {
        arguments: {
          action: "write",
          path: "D:/repo/README.md",
          content: "fabricated final body",
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );
    const fileEdit = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        arguments: {
          path: "D:/repo/src/main.ts",
          old_string: "fabricated old",
          new_string: "fabricated new",
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );
    const applyPatch = resolveFileChangePresentation(
      toolBlock("apply_patch", "", {
        arguments: {
          patch: "*** Begin Patch\n*** Update File: src/main.ts\n-old\n+new\n*** End Patch",
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(fileWrite).toMatchObject({
      interactive: false,
      source: "native_file_mutation",
      files: [{ expandable: false, rows: [] }],
    });
    expect(fileEdit).toMatchObject({
      hasEffectiveChanges: false,
      interactive: false,
      source: "native_file_edit",
      files: [],
    });
    expect(applyPatch).toMatchObject({
      hasEffectiveChanges: false,
      interactive: false,
      source: "native_apply_patch",
      files: [],
    });
  });

  it("maps read results to an independent multi-target read card", () => {
    const presentation = resolveFileAccessPresentation(
      toolBlock(
        "file",
        JSON.stringify({
          action: "read",
          results: [
            { ok: true, lines: [{ line: 4, content: "alpha" }] },
            { ok: true, lines: [{ line: 9, content: "beta" }] },
          ],
        }),
        {
          arguments: {
            action: "read",
            targets: [{ path: "src/a.ts" }, { path: "src/b.ts" }],
          },
        },
      ),
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(presentation).toMatchObject({
      action: "read",
      kind: "file_read",
      items: [
        { path: "src/a.ts", rows: [{ lineNumber: 4, content: "alpha" }] },
        { path: "src/b.ts", rows: [{ lineNumber: 9, content: "beta" }] },
      ],
    });
    expect(presentation?.items.map((item) => item.diffFile?.hunks[0]?.lines)).toEqual([
      [{ content: "alpha", kind: "context", oldLine: 4, newLine: 4 }],
      [{ content: "beta", kind: "context", oldLine: 9, newLine: 9 }],
    ]);
  });

  it("keeps list results structured instead of projecting fake code gutter lines", () => {
    // blocks is one exact native list invocation shared by positive and negative adapter assertions.
    // blocks 是一条精确原生列表调用，由正向与反向适配器断言共同使用。
    const blocks = toolBlock("file", "", {
        arguments: {
          action: "list",
          path: "src",
        },
        host_result: {
          kind: "file_access",
          payload: {
            schema: "vulcan.file_access.v2",
            action: "list",
            path: "D:/repo/src",
            entries: [
              { path: "D:/repo/src/components", kind: "directory", size: 0 },
              { path: "D:/repo/src/main.ts", kind: "file", size: 42 },
            ],
          },
        },
      });
    const presentation = resolveFileAccessPresentation(
      blocks,
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(presentation).toMatchObject({
      action: "list",
      kind: "file_list",
      items: [{
        diffFile: null,
        rows: [
          { content: "[directory] D:/repo/src/components (0 B)", lineNumber: null },
          { content: "[file] D:/repo/src/main.ts (42 B)", lineNumber: null },
        ],
      }],
    });
    expect(resolveFileChangePresentation(
      blocks,
      "completed",
      { workspacePath: "D:/repo" },
    )).toBeNull();
  });

  it("keeps stat metadata outside the code gutter", () => {
    const presentation = resolveFileAccessPresentation(
      toolBlock("file", "", {
        arguments: {
          action: "stat",
          path: "src/main.ts",
        },
        host_result: {
          kind: "file_access",
          payload: {
            schema: "vulcan.file_access.v2",
            action: "stat",
            path: "D:/repo/src/main.ts",
            kind: "file",
            size: 42,
          },
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(presentation?.kind).toBe("file_stat");
    expect(presentation?.items[0]?.diffFile).toBeNull();
    expect(presentation?.items[0]?.rows).toEqual(expect.arrayContaining([
      { content: "kind: file", kind: "context", lineNumber: null },
      { content: "size: 42", kind: "context", lineNumber: null },
    ]));
  });

  it("preserves duplicate read targets so result indexes stay aligned", () => {
    const presentation = resolveFileAccessPresentation(
      toolBlock(
        "file",
        JSON.stringify({
          action: "read",
          results: [
            { ok: true, lines: [{ line: 4, content: "first" }] },
            { ok: true, lines: [{ line: 9, content: "second" }] },
          ],
        }),
        {
          arguments: {
            action: "read",
            targets: [{ path: "src/a.ts" }, { path: "src/a.ts" }],
          },
        },
      ),
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(presentation).toMatchObject({
      kind: "file_read",
      items: [
        { path: "src/a.ts", rows: [{ lineNumber: 4, content: "first" }] },
        { path: "src/a.ts", rows: [{ lineNumber: 9, content: "second" }] },
      ],
    });
  });

  it("uses resource and operation mappings for native file_edit rename results", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [{
              resource_id: "resource-1",
              change: "rename",
              old_path: "src/old.ts",
              new_path: "src/new.ts",
            }],
            extensions: {
              "vulcan.file_edit.v1": {
                operation_results: [{
                  operation_id: "move-1",
                  resource_id: "resource-1",
                }],
              },
            },
          },
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(presentation).toMatchObject({
      kind: "file_change",
      source: "native_file_edit",
      files: [{
        operation: "rename",
        path: "src/new.ts",
        previousPath: "src/old.ts",
        resourceId: "resource-1",
        operationIds: ["move-1"],
      }],
    });
    expect(presentation?.files[0]?.id).toBe("tool-call:t8ci0h:file_edit");
  });

  it("keeps Apply Patch multi-file preview identities through native v2 terminal mapping", () => {
    const preview = resolveFileChangePresentation(
      toolBlock("apply_patch", "", {
        arguments: { patch: "*** Begin Patch\n*** Update File: src/a.ts\n-old\n+new\n*** Add File: src/b.ts\n+created\n*** End Patch" },
      }),
      "running",
      {
        requestPreview: {
          kind: "apply_patch",
          revision: 3,
          affected_resources: ["src/a.ts", "src/b.ts"],
          body_preview: "*** Begin Patch\n*** Update File: src/a.ts\n-old\n+new\n*** Add File: src/b.ts\n+created\n*** End Patch",
          structured_lines: [
            { operation_index: 0, resource_index: 0, side: "old", line_index: 0, content: "old" },
            { operation_index: 0, resource_index: 0, side: "new", line_index: 0, content: "new" },
            { operation_index: 1, resource_index: 1, side: "new", line_index: 0, content: "created" },
          ],
        },
        workspacePath: "D:/repo",
      },
    );
    const completed = resolveFileChangePresentation(
      toolBlock("apply_patch", "", {
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [
              { resource_id: "resource-1", change: "modify", path: "D:/repo/src/a.ts", hunks: [] },
              { resource_id: "resource-2", change: "create", path: "D:/repo/src/b.ts", content: "created\n" },
            ],
            extensions: {
              "vulcan.file_tools.v2": {
                operation_results: [
                  { operation_index: 0, resource_id: "resource-1", request_path: "src/a.ts" },
                  { operation_index: 1, resource_id: "resource-2", request_path: "src/b.ts" },
                ],
              },
            },
          },
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(preview).toMatchObject({
      source: "native_apply_patch",
      files: [
        { operation: "modify", path: "src/a.ts", rows: [{ kind: "delete" }, { kind: "insert" }] },
        { operation: "create", path: "src/b.ts", rows: [{ kind: "insert" }] },
      ],
    });
    expect(completed).toMatchObject({
      source: "native_apply_patch",
      files: [
        { operationIds: ["operation-index:0"], resourceId: "resource-1" },
        { operationIds: ["operation-index:1"], resourceId: "resource-2" },
      ],
    });
    expect(completed?.id).toBe(preview?.id);
    expect(completed?.files.map((file) => file.id)).toEqual(preview?.files.map((file) => file.id));
  });

  it("allows opening the file list of hunkless Apply Patch without inventing diff rows", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("apply_patch", "", {
        arguments: {
          patch: "*** Begin Patch\n*** Update File: src/a.ts\n-old-a\n+new-a\n*** Update File: src/b.ts\n-old-b\n+new-b\n*** End Patch",
        },
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [
              {
                change: "modify",
                path: "D:/repo/src/b.ts",
                inserted_line_count: 1,
                deleted_line_count: 1,
                hunks: [],
              },
              {
                change: "modify",
                path: "D:/repo/src/a.ts",
                inserted_line_count: 1,
                deleted_line_count: 1,
                hunks: [],
              },
            ],
          },
        },
      }),
      "completed",
      { workspacePath: "D:/repo" },
    );

    expect(presentation).toMatchObject({
      interactive: true,
      source: "native_apply_patch",
      totalAdded: 2,
      totalRemoved: 2,
      files: [
        {
          path: "D:/repo/src/b.ts",
          expandable: false,
          rows: [],
        },
        {
          path: "D:/repo/src/a.ts",
          expandable: false,
          rows: [],
        },
      ],
    });
  });

  it("maps native file write change sets without treating them as read-only access", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("file", "", {
        arguments: { action: "write", path: "src/new.ts", content: "export {};\n" },
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [{ resource_id: "resource-1", change: "create", path: "src/new.ts", content: "export {};\n" }],
            extensions: {
              "vulcan.file_tools.v2": {
                operation_results: [{ operation_index: 0, resource_id: "resource-1", request_path: "src/new.ts" }],
                resources: [{
                  resource_id: "resource-1",
                  after_text_format: {
                    encoding: "utf-16le",
                    bom: true,
                    line_ending: "crlf",
                    mixed_line_endings: false,
                    final_newline: true,
                  },
                }],
              },
            },
          },
        },
      }),
      "completed",
      {},
    );

    expect(presentation).toMatchObject({
      source: "native_file_mutation",
      files: [{
        operation: "create",
        readOnly: false,
        rows: [{ content: "export {};" }],
        textFormat: {
          encoding: "utf-16le",
          bom: true,
          lineEnding: "crlf",
          mixedLineEndings: false,
          finalNewline: true,
        },
      }],
    });
  });

  it("shows native file mutations from the public action and path before completion", () => {
    for (const [action, operation] of [
      ["write", "modify"],
      ["delete", "delete"],
      ["move", "rename"],
    ] as const) {
      const presentation = resolveFileChangePresentation(
        toolBlock("file", "", {
          arguments: {
            action,
            path: "src/current.ts",
            ...(action === "move" ? { destination: "src/next.ts" } : {}),
          },
        }),
        "running",
        { workspacePath: "D:/repo" },
      );

      expect(presentation).toMatchObject({
        source: "native_file_mutation",
        state: "running",
        files: [{ operation, path: "src/current.ts", readOnly: false }],
      });
    }
  });

  it("shows the simplified file_edit path before a streamed preview exists", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        arguments: {
          path: "src/simple.ts",
          old_string: "old",
          new_string: "new",
        },
      }),
      "running",
      { workspacePath: "D:/repo" },
    );

    expect(presentation).toMatchObject({
      source: "native_file_edit",
      state: "running",
      files: [{ operation: "modify", path: "src/simple.ts" }],
    });
  });

  it("shows file_edit operation paths before the host change_set arrives", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        arguments: {
          operations: [
            { id: "one", operation: "replace", path: "src/one.ts", old_lines: ["a"], new_lines: ["b"] },
            { id: "two", operation: "replace", path: "src/two.ts", old_lines: ["c"], new_lines: ["d"] },
          ],
        },
      }),
      "running",
      { workspacePath: "D:/repo" },
    );

    expect(presentation).toMatchObject({
      source: "native_file_edit",
      state: "running",
      files: [{ path: "src/one.ts" }, { path: "src/two.ts" }],
    });
  });

  it("maps the bounded streaming preview to multi-file +/- rows", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        arguments: { operations: [] },
      }),
      "running",
      {
        requestPreview: {
          kind: "file_edit",
          revision: 4,
          affected_resources: ["src/one.ts", "src/two.ts"],
          body_preview: "src/one.ts (replace)\nsrc/two.ts (create)",
          structured_lines: [
            { operation_index: 0, resource_index: 0, side: "old", line_index: 0, content: "old" },
            { operation_index: 0, resource_index: 0, side: "new", line_index: 0, content: "new" },
            { operation_index: 1, resource_index: 1, side: "new", line_index: 0, content: "created" },
          ],
        },
        workspacePath: "D:/repo",
      },
    );

    expect(presentation).toMatchObject({
      mode: "preview",
      source: "native_file_edit",
      files: [
        {
          path: "src/one.ts",
          operation: "modify",
          added: 1,
          removed: 1,
          rows: [
            { kind: "delete", content: "old" },
            { kind: "insert", content: "new" },
          ],
        },
        {
          path: "src/two.ts",
          operation: "create",
          added: 1,
          removed: 0,
          rows: [{ kind: "insert", content: "created" }],
        },
      ],
    });
  });

  it("falls back to body preview when structured preview is truncated", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        arguments: { operations: [] },
      }),
      "running",
      {
        requestPreview: {
          kind: "file_edit",
          revision: 5,
          affected_resources: ["src/large.ts"],
          body_preview: "src/large.ts (replace)\n- old\n+ new\n+ tail",
          structured_lines: [
            { operation_index: 0, resource_index: 0, side: "old", line_index: 0, content: "old" },
          ],
          redaction_flags: ["structured_preview_truncated"],
        },
        workspacePath: "D:/repo",
      },
    );

    expect(presentation?.files[0]?.rows).toEqual([
      { content: "old", kind: "delete", lineNumber: 1 },
      { content: "new", kind: "insert", lineNumber: 2 },
      { content: "tail", kind: "insert", lineNumber: 3 },
    ]);
  });

  it("drops failed expected previews and preserves the canonical host error", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        arguments: {
          path: "src/failing.ts",
          old_string: "old",
          new_string: "new",
        },
        host_result: {
          kind: "tool_error_detail",
          payload: {
            status: "failed",
            error: "old_string was not found",
            details: {
              code: "match_not_found",
              message: "The current file no longer contains the requested text.",
            },
          },
        },
      }),
      "failed",
      {
        requestPreview: {
          kind: "file_edit",
          revision: 2,
          affected_resources: ["src/failing.ts"],
          structured_lines: [
            { operation_index: 0, resource_index: 0, side: "old", line_index: 0, content: "old" },
            { operation_index: 0, resource_index: 0, side: "new", line_index: 0, content: "new" },
          ],
        },
        workspacePath: "D:/repo",
      },
    );

    expect(presentation).toMatchObject({
      state: "failed",
      hasEffectiveChanges: false,
      errorDetail: "old_string was not found\nThe current file no longer contains the requested text.",
      errorCode: "match_not_found",
      interactive: true,
      files: [{ path: "src/failing.ts", displayPath: "src/failing.ts", operation: "modify", added: null, removed: null, rows: [] }],
    });
  });

  // V1 failures retain the envelope diagnostic even when an empty changeset shadows legacy host metadata.
  // 即使空变更集覆盖旧式宿主元数据，V1 失败仍保留信封中的诊断。
  it.each([false, true])("retains failed V1 targets with empty changeset=%s", (emptyChangeset) => {
    // input is the original exact-text request, not a successful changeset.
    // input 是原始精确文本请求，不是成功变更集。
    const input = { path: "src/failing.ts", old_string: "old", new_string: "new" };
    // hostResult matches the native FileToolError result envelope.
    // hostResult 对应原生 FileToolError 结果信封。
    const hostResult = { kind: "tool_error_detail", payload: { status: "failed", error: "Exact text not found", details: { code: "match_not_found" } } };
    // changeset deliberately contains no committed files.
    // changeset 特意不包含已提交文件。
    const changeset = { mode: "applied", files: [] };
    // presentation uses the same duplicate argument projection as production toolMetadata.
    // presentation 使用与生产 toolMetadata 相同的参数投影。
    const presentation = resolveFileChangePresentation(toolBlock("file_edit", "", {
      arguments: input, v1_event: true,
      host_result: emptyChangeset ? { kind: "change_set", payload: changeset } : hostResult,
      v1_tool_event: {
        kind: "tool_call", api_call_id: 1, tool_call_id: 1, tool_name: "file_edit", input,
        request_stream_id: "failed-v1-edit", status: "failed",
        output: { model_output: hostResult.payload, host_result: hostResult, is_error: true },
        ...(emptyChangeset ? { changeset } : {}),
      },
    }), "failed", {});
    expect(presentation).toMatchObject({
      errorCode: "match_not_found", errorDetail: "Exact text not found", interactive: true,
      hasEffectiveChanges: false, totalAdded: 0, totalRemoved: 0,
      files: [{ path: "src/failing.ts", rows: [], added: null, removed: null }],
    });
  });

  // Native error envelopes contain both Display-prefixed text and the original structured message.
  // 原生错误信封同时包含 Display 添加前缀后的文本和原始结构化消息。
  it.each([false, true])("deduplicates formatted native errors for V1=%s without dropping context", (v1) => {
    // input and message reproduce the invalid new_text field reported in the UI.
    // input 与 message 复现界面报告的无效 new_text 字段。
    const input = { path: "tui_tests/README.md", new_text: "replacement" };
    const message = "Unknown field new_text in file_edit request.";
    // Each details message tests an exact duplicate, genuinely additional context, or another coded error.
    // 每份详情消息分别验证完全重复、真正的补充内容及另一种错误码的错误。
    for (const detailMessage of [message, "Use new_string instead.", `[another_code] ${message}`]) {
      // hostResult follows from_tool_error_details and FileToolError::fmt exactly.
      // hostResult 精确遵循 from_tool_error_details 与 FileToolError::fmt。
      const hostResult = { kind: "tool_error_detail", payload: {
        error: `[invalid_arguments] ${message}`, details: { code: "invalid_arguments", message: detailMessage },
      } };
      // metadata covers both historical records and the event-native envelope.
      // metadata 同时覆盖历史记录与事件原生信封。
      const metadata = {
        arguments: input, host_result: hostResult,
        ...(v1 ? { v1_event: true, v1_tool_event: {
          kind: "tool_call", api_call_id: 1, tool_call_id: 1, tool_name: "file_edit", input,
          request_stream_id: "duplicate-error", status: "failed",
          output: { model_output: hostResult.payload, host_result: hostResult, is_error: true },
        } } : {}),
      };
      // presentation must keep a single authoritative message unless details add different information.
      // 仅当详情增加不同信息时，presentation 才保留第二条消息。
      const presentation = resolveFileChangePresentation(toolBlock("file_edit", "", metadata), "failed", {});
      expect(presentation?.errorDetail).toBe(detailMessage === message
        ? `[invalid_arguments] ${message}`
        : `[invalid_arguments] ${message}\n${detailMessage}`);
    }
  });

  // Multi-file patch failure exposes attempted resources, never the unapplied preview rows.
  // 多文件补丁失败展示尝试操作的资源，绝不展示未应用的预览差异行。
  it("retains target names and operations when a multi-file patch fails", () => {
    // presentation contains a patch with both edit and create operations.
    // presentation 包含同时具有编辑和创建操作的补丁。
    const presentation = resolveFileChangePresentation(toolBlock("apply_patch", "", {
      arguments: { patch: "*** Begin Patch\n*** Update File: src/a.ts\n@@\n-old\n+new\n*** Add File: src/b.ts\n+created\n*** End Patch" },
      host_result: { kind: "tool_error_detail", payload: { error: "Patch context did not match", details: { code: "patch_context_not_found" } } },
    }), "failed", {});
    expect(presentation).toMatchObject({
      errorCode: "patch_context_not_found", interactive: true, hasEffectiveChanges: false,
      files: [
        { path: "src/a.ts", operation: "modify", rows: [], added: null, removed: null },
        { path: "src/b.ts", operation: "create", rows: [], added: null, removed: null },
      ],
    });
  });

  it("keeps native file_edit presentation ids stable from preview through rename completion", () => {
    const preview = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        arguments: {
          operations: [{ id: "move", operation: "replace", path: "src/old.ts", old_lines: ["old"], new_lines: ["new"] }],
        },
      }),
      "running",
      {
        requestPreview: {
          kind: "file_edit",
          revision: 1,
          affected_resources: ["src/old.ts"],
          structured_lines: [
            { operation_index: 0, resource_index: 0, side: "old", line_index: 0, content: "old" },
            { operation_index: 0, resource_index: 0, side: "new", line_index: 0, content: "new" },
          ],
        },
        workspacePath: null,
      },
    );
    const completed = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        arguments: {
          operations: [{ id: "move", operation: "replace", path: "src/old.ts", old_lines: ["old"], new_lines: ["new"] }],
        },
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [{
              resource_id: "resource-1",
              change: "rename",
              old_path: "D:/repo/src/old.ts",
              new_path: "D:/repo/src/new.ts",
            }],
          },
        },
      }),
      "completed",
      {},
    );

    expect(preview?.id).toBe(completed?.id);
    expect(preview?.files[0]?.id).toBe(completed?.files[0]?.id);
  });

  it("prefers the longest unique request-path suffix for nested multi-file results", () => {
    const argumentsMetadata = {
      arguments: {
        operations: [
          { id: "short", operation: "replace", path: "src/a.ts", old_lines: ["a"], new_lines: ["b"] },
          { id: "nested", operation: "replace", path: "lib/src/a.ts", old_lines: ["a"], new_lines: ["c"] },
        ],
      },
    };
    const preview = resolveFileChangePresentation(
      toolBlock("file_edit", "", argumentsMetadata),
      "running",
      {
        requestPreview: {
          kind: "file_edit",
          revision: 1,
          affected_resources: ["src/a.ts", "lib/src/a.ts"],
        },
      },
    );
    const completed = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        ...argumentsMetadata,
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [
              { change: "modify", path: "D:/repo/lib/src/a.ts", hunks: [] },
              { change: "modify", path: "D:/repo/src/a.ts", hunks: [] },
            ],
          },
        },
      }),
      "completed",
      {},
    );

    expect(new Set(completed?.files.map((file) => file.id))).toEqual(
      new Set(preview?.files.map((file) => file.id)),
    );
  });

  it("keeps the presentation id stable when canonical files reorder", () => {
    const baseMetadata = {
      arguments: {
        operations: [
          { id: "one", operation: "replace", path: "src/one.ts", old_lines: ["a"], new_lines: ["b"] },
          { id: "two", operation: "replace", path: "src/two.ts", old_lines: ["c"], new_lines: ["d"] },
        ],
      },
    };
    const first = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        ...baseMetadata,
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [
              { resource_id: "resource-1", change: "modify", path: "src/one.ts", hunks: [] },
              { resource_id: "resource-2", change: "modify", path: "src/two.ts", hunks: [] },
            ],
          },
        },
      }),
      "completed",
      {},
    );
    const reordered = resolveFileChangePresentation(
      toolBlock("file_edit", "", {
        ...baseMetadata,
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [
              { resource_id: "resource-2", change: "modify", path: "src/two.ts", hunks: [] },
              { resource_id: "resource-1", change: "modify", path: "src/one.ts", hunks: [] },
            ],
          },
        },
      }),
      "completed",
      {},
    );

    expect(first?.id).toBe(reordered?.id);
  });

  it("keeps arbitrary change_set producers on the generic compatibility adapter", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("lua_skill_custom_edit", "", {
        host_result: {
          kind: "change_set",
          payload: {
            mode: "applied",
            files: [{ change: "modify", path: "src/custom.lua", hunks: [] }],
          },
        },
      }),
      "completed",
      {},
    );

    expect(presentation).toMatchObject({ source: "host_change_set", files: [{ path: "src/custom.lua" }] });
  });

  it("does not infer a native operation from an unrelated tool-name substring", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("not_vulcan_file_edit_helper", "", { arguments: { file: "src/example.ts" } }),
      "running",
      {},
    );

    expect(presentation).toBeNull();
  });
});
