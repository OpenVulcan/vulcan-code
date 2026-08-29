import { describe, expect, it } from "vitest";
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

  it("keeps a hunkless completed Apply Patch closed instead of rebuilding request rows", () => {
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
      interactive: false,
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
      files: [],
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
