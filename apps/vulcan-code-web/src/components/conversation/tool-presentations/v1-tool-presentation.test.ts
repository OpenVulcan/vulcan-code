import { describe, expect, it } from "vitest";

import {
  buildV1ChangeSetFilePresentation,
  resolveV1ToolPresentation,
} from "./v1-tool-presentation";
import type { ToolCallDisplayItem } from "@/service/conversation-event-display-projection";
import type {
  PersistentChangeSet,
  ToolPayload,
} from "@/service/conversation-event-window-store";

/** Builds one complete V1 tool display item without using legacy records.
 * 在不使用旧记录的情况下构建一条完整 V1 工具显示项。 */
function toolItem(options: {
  seq?: number;
  apiCallId?: number;
  toolCallId?: number;
  toolName?: string;
  input?: ToolPayload;
  output?: ToolPayload;
  changeset?: PersistentChangeSet;
  status?: ToolCallDisplayItem["tool_call"]["status"];
} = {}): ToolCallDisplayItem {
  // seq is the only presentation-order identity.
  // seq 是唯一的展示顺序身份。
  const seq = options.seq ?? 1;
  // apiCallId keeps success and failure inside their original physical request.
  // apiCallId 让成功与失败保持在原始物理请求内。
  const apiCallId = options.apiCallId ?? 11;
  return {
    kind: "tool_call",
    seq,
    event_revision: 1,
    turn_id: 7,
    created_at_ms: 1,
    updated_at_ms: 2,
    visibility: "provider",
    group: { turn_id: 7, api_call_id: apiCallId },
    tool_call: {
      kind: "tool_call",
      api_call_id: apiCallId,
      tool_call_id: options.toolCallId ?? seq + 100,
      tool_name: options.toolName ?? "file_edit",
      input: options.input ?? { path: "D:/project/README.md" },
      request_stream_id: `v1-presentation-tool-${options.toolCallId ?? seq + 100}`,
      output: options.output,
      changeset: options.changeset,
      status: options.status ?? "completed",
    },
  };
}

/** Builds the regression ChangeSet for +140/-208 and persisted format metadata.
 * 构建覆盖 +140/-208 与持久化格式元数据的回归 ChangeSet。 */
function lifecycleChangeSet(): PersistentChangeSet {
  return {
    mode: "applied",
    summary: "Lifecycle regression",
    diagnostics: [{ level: "warning", message: "retained snippets" }],
    files: [
      {
        change: "create",
        path: "README.new.md",
        hunks: [],
        lifecycle_content: {
          mode: "full",
          content: Array.from({ length: 140 }, (_value, index) => `created ${index + 1}`).join("\n"),
          total_line_count: 140,
        },
        after_text_format: { encoding: "utf-8", line_ending: "LF" },
      },
      {
        change: "delete",
        path: "README.old.md",
        hunks: [],
        lifecycle_content: {
          mode: "truncated",
          head: "deleted 1\ndeleted 2",
          tail: "deleted 207\ndeleted 208",
          total_line_count: 208,
        },
        before_text_format: { encoding: "utf-8", line_ending: "LF" },
      },
    ],
  };
}

describe("V1 event-native tool presentation", () => {
  it("keeps +140/-208 totals consistent with retained and explicitly omitted rows", () => {
    const presentation = resolveV1ToolPresentation(toolItem({ changeset: lifecycleChangeSet() }));
    if (presentation.kind !== "file_change") throw new Error(`expected file_change, got ${presentation.kind}`);

    expect([presentation.additions, presentation.deletions]).toEqual([140, 208]);
    const created = presentation.files[0];
    const deleted = presentation.files[1];
    expect(created?.rows.map((row) => row.content)).toContain("created 1");
    expect(created?.rows).toHaveLength(140);
    expect(created?.rows.find((row) => row.kind === "omitted")).toBeUndefined();
    expect(deleted?.rows.map((row) => row.content)).toEqual([
      "deleted 1",
      "deleted 2",
      "省略 204 行",
      "deleted 207",
      "deleted 208",
    ]);
    expect(deleted?.rows.reduce((total, row) => total + row.logicalLineCount, 0)).toBe(208);
    expect(deleted?.beforeFormat).toBe("utf-8 · LF");
    expect(presentation.diagnostics).toEqual([{ level: "warning", message: "retained snippets" }]);
  });

  it("renders modify hunks in producer order with real deleted and inserted lines", () => {
    const file = buildV1ChangeSetFilePresentation({
      change: "modify",
      path: "src/main.ts",
      hunks: [{
        before: "before context",
        after: "after context",
        deleted: [{ line: 10, content: "const oldValue = 1;" }],
        inserted: [{ line: 10, content: "const newValue = 2;" }],
      }],
      before_text_format: { encoding: "utf-8", line_ending: "LF" },
      after_text_format: { encoding: "utf-8", line_ending: "LF" },
    });

    expect(file.rows.map((row) => [row.kind, row.lineNumber, row.content])).toEqual([
      ["context", null, "before context"],
      ["deleted", 10, "const oldValue = 1;"],
      ["added", 10, "const newValue = 2;"],
      ["context", null, "after context"],
    ]);
    expect([file.additions, file.deletions]).toEqual([1, 1]);
    expect([file.beforeFormat, file.afterFormat]).toEqual(["utf-8 · LF", "utf-8 · LF"]);
  });

  it("keeps both rename paths and lets rename hunks own content changes", () => {
    const file = buildV1ChangeSetFilePresentation({
      change: "rename",
      old_path: "docs/old-name.md",
      new_path: "docs/new-name.md",
      hunks: [{
        before: "unchanged before",
        after: "unchanged after",
        deleted: [{ line: 4, content: "old heading" }],
        inserted: [{ line: 4, content: "new heading" }],
      }],
      lifecycle_content: {
        mode: "full",
        content: "old heading\n",
        total_line_count: 1,
      },
      before_text_format: { encoding: "utf-8", line_ending: "CRLF" },
      after_text_format: { encoding: "utf-8", line_ending: "LF" },
    });

    expect(file).toMatchObject({
      path: "docs/new-name.md",
      oldPath: "docs/old-name.md",
      newPath: "docs/new-name.md",
      displayPath: "docs/old-name.md → docs/new-name.md",
      additions: 1,
      deletions: 1,
      beforeFormat: "utf-8 · CRLF",
      afterFormat: "utf-8 · LF",
    });
    expect(file.rows.map((row) => [row.kind, row.content])).toEqual([
      ["context", "unchanged before"],
      ["deleted", "old heading"],
      ["added", "new heading"],
      ["context", "unchanged after"],
    ]);
  });

  it("renders pure rename lifecycle text without inventing line changes", () => {
    const file = buildV1ChangeSetFilePresentation({
      change: "rename",
      old_path: "src/old.ts",
      new_path: "src/new.ts",
      hunks: [],
      lifecycle_content: {
        mode: "full",
        content: "export const value = 1;\n",
        total_line_count: 1,
      },
    });

    expect(file.displayPath).toBe("src/old.ts → src/new.ts");
    expect([file.additions, file.deletions]).toEqual([0, 0]);
    expect(file.rows).toEqual([{
      kind: "context",
      lineNumber: 1,
      content: "export const value = 1;",
      logicalLineCount: 0,
    }]);
  });

  it("represents omitted lifecycle bodies explicitly without inventing file text", () => {
    const file = buildV1ChangeSetFilePresentation({
      change: "delete",
      path: "binary.dat",
      hunks: [],
      lifecycle_content: { mode: "omitted", total_line_count: 24 },
    });

    expect(file.rows).toEqual([{
      kind: "omitted",
      lineNumber: null,
      content: "正文由产出方明确省略",
      logicalLineCount: 24,
    }]);
    expect(file.deletions).toBe(24);
  });

  it("hides the icon slot for exact directory-list tools", () => {
    const nativeList = resolveV1ToolPresentation(toolItem({
      toolName: "file",
      input: { action: "list", path: "D:/project" },
      output: { markdown: "- src" },
    }));
    const legacyList = resolveV1ToolPresentation(toolItem({
      toolName: "vulcan_file_list",
      input: { path: "D:/project" },
      output: { markdown: "- src" },
    }));

    expect(nativeList).toMatchObject({ kind: "file_access", action: "list", icon: null });
    expect(legacyList).toMatchObject({ kind: "file_access", action: "list", icon: null });
  });

  it("keeps failed and successful tools in caller-supplied seq and API grouping", () => {
    const items = [
      toolItem({ seq: 20, apiCallId: 51, toolCallId: 201, toolName: "shell_command", status: "failed" }),
      toolItem({ seq: 21, apiCallId: 51, toolCallId: 202, toolName: "shell_command", status: "completed" }),
    ];

    const presentations = items.map(resolveV1ToolPresentation);

    expect(presentations.map((presentation) => [presentation.seq, presentation.apiCallId, presentation.state])).toEqual([
      [20, 51, "failed"],
      [21, 51, "completed"],
    ]);
  });

  it("resolves hydration and live copies of the same event to one equivalent card", () => {
    const liveItem = toolItem({
      seq: 9,
      toolName: "shell_command",
      input: { command: "echo ok" },
      output: { text: "ok" },
    });
    const hydratedItem = structuredClone(liveItem);

    expect(resolveV1ToolPresentation(hydratedItem)).toEqual(resolveV1ToolPresentation(liveItem));
  });

  it("matches native tools by exact id and sends unknown ids to the generic card", () => {
    const native = resolveV1ToolPresentation(toolItem({ toolName: "create_goal" }));
    const unknown = resolveV1ToolPresentation(toolItem({ toolName: "create_goal_extra" }));

    expect(native).toMatchObject({ kind: "native", family: "goal", title: "创建目标" });
    expect(unknown).toMatchObject({ kind: "generic", title: "create_goal_extra" });
  });

  it("projects model-extension tools with localized titles and JSON payload blocks", () => {
    const presentation = resolveV1ToolPresentation(toolItem({
      toolName: "get_model_extension_parameters",
      input: { capability: "video_generate" },
      output: { target_id: "MiniMax-Hailuo-2.3", duration_seconds: 6 },
    }));

    expect(presentation).toMatchObject({
      kind: "generic",
      titleKey: "toolPresentation.modelExtension.parameters",
      inputMarkdown: "```json\n{\n  \"capability\": \"video_generate\"\n}\n```",
      outputMarkdown: "```json\n{\n  \"target_id\": \"MiniMax-Hailuo-2.3\",\n  \"duration_seconds\": 6\n}\n```",
    });
  });

  it("renders exact CodeKit Markdown tools from their own event payload", () => {
    const presentation = resolveV1ToolPresentation(toolItem({
      toolName: "vulcan_codekit_rg",
      input: { dir: "D:/project", rg_pattern: "ToolCall" },
      output: { markdown: "# RG results\n\n- one match" },
    }));

    expect(presentation).toMatchObject({
      kind: "markdown",
      title: "搜索代码",
      markdown: "# RG results\n\n- one match",
      targets: [],
    });
  });

  it("keeps shell command and terminal result on the single event card", () => {
    const presentation = resolveV1ToolPresentation(toolItem({
      toolName: "shell_command",
      input: { command: "echo ok" },
      output: { text: "ok" },
      status: "failed",
    }));

    expect(presentation).toMatchObject({
      kind: "shell",
      command: "echo ok",
      outputMarkdown: "ok",
      state: "failed",
    });
  });

  it("keeps the exact request path visible before a file mutation has a ChangeSet", () => {
    const presentation = resolveV1ToolPresentation(toolItem({
      toolName: "file_edit",
      input: { path: "D:/project/src/main.ts", old_string: "old", new_string: "new" },
      status: "running",
    }));

    expect(presentation).toMatchObject({
      kind: "file_change",
      requestTarget: "D:/project/src/main.ts",
      state: "running",
      files: [],
    });
  });

  it("projects exact file and file_edit events into their native V1 cards", () => {
    const fileRead = resolveV1ToolPresentation(toolItem({
      toolName: "file",
      input: { action: "read", path: "D:/project/src/main.ts" },
      output: { text: "export const value = 1;" },
    }));
    const fileEdit = resolveV1ToolPresentation(toolItem({
      toolName: "file_edit",
      changeset: lifecycleChangeSet(),
    }));

    expect(fileRead).toMatchObject({
      kind: "file_access",
      action: "read",
      targets: ["D:/project/src/main.ts"],
      markdown: "export const value = 1;",
    });
    expect(fileEdit).toMatchObject({
      kind: "file_change",
      mode: "applied",
      additions: 140,
      deletions: 208,
    });
  });

  it("keeps ask and dynamic subagent tools self-contained without legacy detail lookup", () => {
    const ask = resolveV1ToolPresentation(toolItem({
      toolName: "ask",
      input: {
        questions: [{
          header: "范围",
          question: "继续执行吗？",
          options: [{ label: "继续", description: "继续当前方案" }],
        }],
      },
      output: {
        host_result: {
          kind: "ask_question",
          payload: { answers: [["继续"]] },
        },
      },
    }));
    const subagent = resolveV1ToolPresentation(toolItem({
      toolName: "review-agent",
      input: { prompt: "审核持久化契约" },
      output: { model_output: "审核完成" },
    }));

    expect(ask).toMatchObject({ kind: "generic", toolName: "ask", title: "ask" });
    expect(ask.kind === "generic" ? ask.inputMarkdown : "").toContain("继续执行吗");
    expect(ask.kind === "generic" ? ask.outputMarkdown : "").toContain("ask_question");
    expect(subagent).toMatchObject({
      kind: "generic",
      toolName: "review-agent",
      title: "review-agent",
    });
    expect(subagent.kind === "generic" ? subagent.outputMarkdown : "").toContain("审核完成");
  });
});
