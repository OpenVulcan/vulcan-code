import { describe, expect, it } from "vitest";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { hydrateFileAccessPresentation, resolveFileChangePresentation } from "./file-change-presentation";

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
  it("labels an implicit bounded read from the first line", () => {
    const presentation = resolveFileChangePresentation(
      toolBlock("file", "", {
        arguments: {
          action: "read",
          targets: [{ path: "src/a.ts" }],
        },
      }),
      "completed",
      {},
      { sessionId: "session-1", recordId: "detail-implicit" },
    );

    expect(presentation?.files[0]?.readRange).toBe("L1-…");
  });

  it("shows requested ranges and hydrates multi-file read results on expansion", () => {
    const presentation = resolveFileChangePresentation(
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
      interactive: true,
      files: [
        { path: "src/a.ts", readRange: "L10-L12, L30", rows: [] },
        { path: "src/b.ts", readRange: "L40-L41", rows: [] },
      ],
    });

    const hydrated = hydrateFileAccessPresentation(presentation!, {
      body: JSON.stringify({
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
      }),
    });

    expect(hydrated).toMatchObject({
      interactive: true,
      files: [
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

  it("maps read results to the shared multi-file card shell", () => {
    const presentation = resolveFileChangePresentation(
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
      accessAction: "read",
      kind: "file_change",
      source: "native_file",
      files: [
        { path: "src/a.ts", readOnly: true, rows: [{ lineNumber: 4, content: "alpha" }] },
        { path: "src/b.ts", readOnly: true, rows: [{ lineNumber: 9, content: "beta" }] },
      ],
    });
    expect(presentation?.files.map((file) => file.diffFile?.hunks[0]?.lines)).toEqual([
      [{ content: "alpha", kind: "context", oldLine: 4, newLine: 4 }],
      [{ content: "beta", kind: "context", oldLine: 9, newLine: 9 }],
    ]);
  });

  it("preserves duplicate read targets so result indexes stay aligned", () => {
    const presentation = resolveFileChangePresentation(
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
      source: "native_file",
      files: [
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
