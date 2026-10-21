// Behaviour of the tool-card readers: lifecycle mapping, card routing, diff numbering and output parsing.
// 工具卡读取器的行为：生命周期映射、卡片路由、差异编号与输出解析。
import { describe, expect, it } from "vitest";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import type { PersistentChangeSetFile } from "@/service/conversation-event-window-store";
import { askAnswers, askQuestions } from "./cards/ask-card";
import { CodeModeCard, firstCodeLine, readCodeModeObservation } from "./cards/code-mode-card";
import { commandOutputFromText } from "./cards/command-card";
import { documentBody } from "./cards/document-card";
import { listEntries, readLines, requestPreviewStat, statFacts } from "./cards/file-cards";
import { toolNameParts } from "./cards/generic-card";
import { knowledgeEdits } from "./cards/knowledge-write-card";
import { codeLines, luaSessionBody } from "./cards/lua-card";
import { goalReport, todoItems } from "./cards/plan-cards";
import { extractReport, searchReport, sourceParts, stripReportHeading } from "./cards/provider-card";
import { formatToolDuration, readToolCall, toolCardState } from "./tool-call";
import { resolveToolCard, toolCallFamily } from "./tool-call-card";
import { readRequestPreview, requestToolCall } from "./tool-request";
import { summarizeToolSequence, toolSequenceEntry } from "./tool-sequence";
import { fileDiffRows, fileFirstEditLines, fileLineCounts } from "./views/change-set";
import { parseTerminalText } from "./views/terminal-view";

// toolEvent builds a minimal completed tool-call event for routing checks.
// toolEvent 为路由检查构建一条最小的已完成工具调用事件。
function toolEvent(toolName: string, input: Record<string, unknown>, host: { kind: string; payload: unknown } | null = null): ToolCallEvent {
  return {
    api_call_id: 1,
    input,
    kind: "tool_call",
    output: { host_result: host, is_error: false, model_output: "", tool_call_id: "call-1", tool_name: toolName },
    request_stream_id: "stream-1",
    status: "completed",
    tool_call_id: 1,
    tool_name: toolName,
  } as unknown as ToolCallEvent;
}

describe("tool call reading", () => {
  it("writes durations at the precision a reader needs", () => {
    expect(formatToolDuration(38)).toBe("38 ms");
    expect(formatToolDuration(6_400)).toBe("6.4 s");
    expect(formatToolDuration(125_000)).toBe("2m 05s");
  });

  it("maps every durable lifecycle onto one card state", () => {
    expect(toolCardState("prepared", false)).toBe("running");
    expect(toolCardState("running", false)).toBe("running");
    expect(toolCardState("waiting_user", false)).toBe("waiting");
    expect(toolCardState("completed", false)).toBe("done");
    expect(toolCardState("completed", true)).toBe("failed");
    expect(toolCardState("failed", false)).toBe("failed");
    expect(toolCardState("interrupted", false)).toBe("stopped");
  });

  it("derives duration only when both timestamps are known", () => {
    const timed = { ...toolEvent("shell_command", {}), finished_at_ms: 1500, started_at_ms: 1000 } as ToolCallEvent;
    expect(readToolCall(timed).durationMs).toBe(500);
    expect(readToolCall(toolEvent("shell_command", {})).durationMs).toBeNull();
  });

  it("routes calls by what they did and leaves unknown tools to the generic card", () => {
    expect(toolCallFamily(toolEvent("file", { action: "read", path: "a.ts" }))).toBe("read");
    expect(toolCallFamily(toolEvent("file", { action: "list", path: "." }))).toBe("read");
    expect(toolCallFamily(toolEvent("file", { action: "delete", path: "a.ts" }))).toBe("edit");
    expect(toolCallFamily(toolEvent("file_edit", { path: "a.ts" }))).toBe("edit");
    expect(toolCallFamily(toolEvent("anything", {}, { kind: "command_run", payload: {} }))).toBe("run");
    expect(toolCallFamily(toolEvent("terminal_session", { action: "read" }))).toBe("run");
    expect(toolCallFamily(toolEvent("spawn_agent", { message: "work" }))).toBe("agent");
    expect(toolCallFamily(toolEvent("read_media", { path: "a.png" }))).toBe("read");
    expect(toolCallFamily(toolEvent("mcp_github_create_issue", {}))).toBe("other");
  });

  it("splits MCP tool names into server and tool", () => {
    expect(toolNameParts("mcp__github__create_issue")).toEqual({ server: "github", tool: "create_issue" });
    expect(toolNameParts("mcp_github_create_issue")).toEqual({ server: "MCP", tool: "github_create_issue" });
    expect(toolNameParts("shell_command").server).toBeNull();
  });
});

describe("change-set rows", () => {
  it("numbers context on both sides across hunks that shift the file", () => {
    const file: PersistentChangeSetFile = {
      change: "modify",
      hunks: [
        { after: "c", before: "a", deleted: [{ content: "b", line: 2 }], inserted: [{ content: "B1", line: 2 }, { content: "B2", line: 3 }] },
        { after: "", before: "x", deleted: [{ content: "y", line: 10 }], inserted: [{ content: "Y", line: 11 }] },
      ],
      path: "a.txt",
    };
    const rows = fileDiffRows(file);
    expect(rows[0]).toMatchObject({ kind: "context", newLine: 1, oldLine: 1, text: "a" });
    expect(rows.find((row) => row.text === "c")).toMatchObject({ newLine: 4, oldLine: 3 });
    expect(rows.find((row) => row.text === "x")).toMatchObject({ newLine: 10, oldLine: 9 });
    expect(fileLineCounts(file)).toEqual({ added: 3, removed: 2 });
    expect(fileFirstEditLines(file)).toEqual({ start: 2, end: 3 });
  });

  it("locates a pure deletion at the line now in its place and nothing for files without hunks", () => {
    const deletion: PersistentChangeSetFile = { change: "modify", hunks: [{ after: "", before: "", deleted: [{ content: "z", line: 7 }], inserted: [] }], path: "a.txt" };
    expect(fileFirstEditLines(deletion)).toEqual({ start: 7, end: 7 });
    expect(fileFirstEditLines({ change: "create", hunks: [], path: "b.txt" })).toBeNull();
  });
});

describe("output parsing", () => {
  it("reads numbered source lines from the host offset or after the opening fence", () => {
    const output = "# File: a.py\n\nLines: 9 | Showing: 4-5\n\n```python\nL4: four\nL5: \n```\n\nTruncated. Continue with start_line=6.\n";
    const expected = [{ number: 4, text: "four" }, { number: 5, text: "" }];
    expect(readLines(output, 6)).toEqual(expected);
    expect(readLines(output, null)).toEqual(expected);
  });

  it("lists directory entries with kind and size", () => {
    const entries = listEntries("- `src` · directory\n- `src/a.ts` · file · 120 bytes\n\nTruncated.");
    expect(entries).toEqual([
      { directory: true, path: "src", size: null },
      { directory: false, path: "src/a.ts", size: 120 },
    ]);
  });

  it("reads padded and long-delimited code spans the runtime writes around paths and facts", () => {
    expect(listEntries("- ` D:/repo/a.ts ` · file · 3 bytes\n- `` odd`name `` · file")).toEqual([
      { directory: false, path: "D:/repo/a.ts", size: 3 },
      { directory: false, path: "odd`name", size: null },
    ]);
    expect(statFacts("Type: file\nEncoding: ` utf-8 `\n").map((fact) => fact.value)).toEqual(["file", "utf-8"]);
  });

  it("joins stdout and stderr sections without the command header", () => {
    const text = "Command: ls\nExit code: 0\n\n[stdout]\none\ntwo\n\n[stderr]\nwarn\n";
    expect(commandOutputFromText(text)).toBe("one\ntwo\nwarn");
  });

  it("keeps foreground tone and weight and skips extended color arguments", () => {
    const spans = parseTerminalText("\u001b[1;31merr\u001b[0m ok \u001b[38;5;196mx\u001b[39m");
    expect(spans[0]).toEqual({ bold: true, text: "err", tone: "t-bad" });
    expect(spans[1]).toEqual({ bold: false, text: " ok x", tone: null });
  });
});

describe("planning and document results", () => {
  it("parses the goal report including an unknown usage and an unset goal", () => {
    const report = goalReport("# Goal\n\nStatus: active\n\n## Objective\n\nShip the cards\nwith tests\n\nTokens used: unknown\n\nToken budget: 5000\n\nElapsed seconds: 90\n");
    expect(report).toEqual({ elapsedSeconds: 90, objective: "Ship the cards\nwith tests", status: "active", tokenBudget: 5000, tokensUsed: null });
    expect(goalReport("# Goal\n\nNo goal is set.\n")?.status).toBeNull();
    expect(goalReport("Todolist updated")).toBeNull();
  });

  it("keeps only well-formed checklist steps", () => {
    expect(todoItems([{ status: "completed", step: "a" }, { status: "later", step: "b" }, { step: "c" }, "d"])).toEqual([{ status: "completed", step: "a" }]);
  });

  it("unwraps a loaded skill and routes planning and document tools", () => {
    expect(documentBody("<skill_content name=\"x\">\n# X\n\nBody\n</skill_content>")).toBe("# X\n\nBody");
    expect(documentBody("plain")).toBe("plain");
    expect(toolCallFamily(toolEvent("todolist", { items: [] }))).toBe("plan");
    expect(toolCallFamily(toolEvent("update_goal", { status: "complete" }))).toBe("plan");
    expect(toolCallFamily(toolEvent("knowledge_search", { queries: ["a"] }))).toBe("search");
    expect(toolCallFamily(toolEvent("vulcan-codekit-rg", {}))).toBe("search");
  });
});

describe("provider results", () => {
  it("reads ranked search hits with escaped titles and multi-line snippets", () => {
    const report = searchReport("# Search: `rust async`\n\n1. [Async \\[book\\]](<https://rust-lang.github.io/async-book/>)\n\nFirst line\nsecond line\n\n2. [Tokio](<https://tokio.rs/a%20b>)\n\nRuntime\n");
    expect(report?.query).toBe("rust async");
    expect(report?.hits).toEqual([
      { rank: 1, snippet: "First line\nsecond line", title: "Async [book]", url: "https://rust-lang.github.io/async-book/" },
      { rank: 2, snippet: "Runtime", title: "Tokio", url: "https://tokio.rs/a%20b" },
    ]);
    expect(searchReport("# Search: `q`\n\nNo results.\n")?.hits).toEqual([]);
    expect(searchReport("# Search: `a`\n\n# Search: `b`\n")).toBeNull();
  });

  it("separates extracted pages, their image lists and failed sources", () => {
    const report = extractReport("# Extracted pages\n\n## [https://a.test/x](<https://a.test/x>)\n\n## Inner heading\n\nBody\n\nImages:\n- [https://a.test/i.png](<https://a.test/i.png>)\n\n## [https://b.test/](<https://b.test/>)\n\nSecond\n\n## Failed sources\n\n- [https://c.test/](<https://c.test/>): timeout\n");
    expect(report?.pages).toEqual([
      { content: "## Inner heading\n\nBody", imageCount: 1, url: "https://a.test/x" },
      { content: "Second", imageCount: 0, url: "https://b.test/" },
    ]);
    expect(report?.failures).toEqual([{ error: "timeout", url: "https://c.test/" }]);
  });

  it("splits source addresses and drops headings the card already names", () => {
    expect(sourceParts("https://www.example.test/docs?q=1")).toEqual({ host: "example.test", rest: "/docs?q=1" });
    expect(sourceParts("not a url")).toEqual({ host: "not a url", rest: "" });
    expect(stripReportHeading("# image understanding\n\nA cat")).toBe("A cat");
    expect(stripReportHeading("# Result\n\nText")).toBe("Text");
  });

  it("routes model extensions by what they produce", () => {
    expect(toolCallFamily(toolEvent("web_search", { query: "q" }))).toBe("web");
    expect(toolCallFamily(toolEvent("image_understand", { prompt: "p" }))).toBe("read");
    expect(toolCallFamily(toolEvent("image_generate", { args: { prompt: "p" }, model: "m/x" }))).toBe("media");
  });
});

describe("tool sequence summary", () => {
  it("counts families in fixed order with unsuccessful and active calls", () => {
    const summary = summarizeToolSequence([
      toolSequenceEntry(toolEvent("file_edit", { path: "a.ts" }), false),
      toolSequenceEntry(toolEvent("file_read", { path: "a.ts" }), false),
      toolSequenceEntry({ ...toolEvent("shell_command", { command: "x" }), status: "interrupted" } as ToolCallEvent, false),
      toolSequenceEntry({ ...toolEvent("shell_command", { command: "y" }), status: "running" } as ToolCallEvent, true),
      toolSequenceEntry(undefined, false),
    ]);
    expect(summary).toEqual({ active: 1, families: [["read", 1], ["edit", 1], ["run", 2], ["other", 1]], unsuccessful: 1 });
  });
});

describe("lua, questions and knowledge writes", () => {
  it("numbers submitted Lua and drops the session heading", () => {
    expect(codeLines("local a = 1\r\nreturn a\n")).toEqual([{ number: 1, text: "local a = 1" }, { number: 2, text: "return a" }]);
    expect(luaSessionBody("# Lua session: eval\n\nSession ID: `l`\n")).toBe("Session ID: `l`\n");
  });

  it("matches answers to questions by header, including unanswered ones and headers with colons", () => {
    const questions = askQuestions([
      { header: "Scope: files", question: "Which files?", options: [{ label: "All", description: "" }] },
      { header: "Mode", question: "Which mode?" },
      { question: "missing header" },
    ]);
    expect(questions).toEqual([
      { header: "Scope: files", options: ["All"], question: "Which files?" },
      { header: "Mode", options: [], question: "Which mode?" },
    ]);
    expect(askAnswers("## User Answers\n\n- Scope: files: All\n- Mode: Unanswered\n\nContinue with these answers.", questions)).toEqual(["All", null]);
    expect(askAnswers("Todolist updated", questions)).toBeNull();
  });

  it("keeps only complete passage edits", () => {
    expect(knowledgeEdits([{ newText: "b", oldText: "a" }, { oldText: "c" }, "x"])).toEqual([{ newText: "b", oldText: "a" }]);
  });

  it("routes scripts, questions, knowledge writes, memory and configuration", () => {
    expect(toolCallFamily(toolEvent("runlua", { code: "return 1", task: "t" }))).toBe("run");
    expect(toolCallFamily(toolEvent("runlua_session", { action: "list" }))).toBe("run");
    expect(toolCallFamily(toolEvent("ask", { questions: [] }))).toBe("agent");
    expect(toolCallFamily(toolEvent("knowledge_write", { action: "create", libraryId: "l" }))).toBe("edit");
    expect(toolCallFamily(toolEvent("vmm_memory_search", { queries: ["q"] }))).toBe("search");
    expect(toolCallFamily(toolEvent("runtime_config", { action: "describe" }))).toBe("read");
  });
});

describe("code mode", () => {
  it("routes exec and wait by their argument shape and leaves look-alike tools to the generic card", () => {
    expect(resolveToolCard(readToolCall(toolEvent("exec", { code: "text(1)" }))).Card).toBe(CodeModeCard);
    expect(resolveToolCard(readToolCall(toolEvent("wait", { cell_id: "3" }))).Card).toBe(CodeModeCard);
    expect(toolCallFamily(toolEvent("exec", { code: "text(1)" }))).toBe("run");
    expect(resolveToolCard(readToolCall(toolEvent("wait", { seconds: 3 }))).Card).not.toBe(CodeModeCard);
  });

  it("splits status, notifications and printed text, and names scripts past the pragma", () => {
    expect(readCodeModeObservation("Script running with cell ID 7\nWall time 0.5 seconds\nNotifications:\nhalfway\nstill going\nOutput:\npartial")).toEqual({
      cellId: "7", images: [], notifications: ["halfway", "still going"], output: "partial", status: "yielded",
    });
    expect(readCodeModeObservation("Script failed\nWall time 0.0 seconds\nOutput:\nScript error:\nboom")).toMatchObject({
      cellId: null, output: "Script error:\nboom", status: "failed",
    });
    expect(readCodeModeObservation("Invalid `exec` arguments")).toMatchObject({ output: "Invalid `exec` arguments", status: null });
    expect(firstCodeLine("// @exec: {\"yield_time_ms\": 100}\n\nconst a = 1;\n")).toBe("const a = 1;");
  });

  it("separates the images a cell returned from its text", () => {
    const envelope = JSON.stringify({
      $vulcan_tool_result_media: 1,
      content: [
        { text: "Script completed\nWall time 0.1 seconds\nOutput:\n", type: "text" },
        { text: "chart ready", type: "text" },
        { data: "iVBORw0KGgo=", mimeType: "image/png", type: "image" },
      ],
    });
    expect(readCodeModeObservation(envelope)).toEqual({
      cellId: null,
      images: [{ data: "iVBORw0KGgo=", mimeType: "image/png" }],
      notifications: [],
      output: "chart ready",
      status: "completed",
    });
  });
});

describe("tool requests still being written", () => {
  it("groups structured preview lines by the file they belong to", () => {
    const preview = readRequestPreview({
      affected_resources: ["a.ts", "b.ts"],
      kind: "apply_patch",
      progress_fields: { new_line_count: 1, old_line_count: 1 },
      redaction_flags: ["structured_preview_truncated"],
      structured_lines: [
        { content: "old", line_index: 0, operation_index: 0, resource_index: 0, side: "old" },
        { content: "new", line_index: 0, operation_index: 1, resource_index: 1, side: "new" },
        { content: "bad", resource_index: 5, side: "new" },
        { content: "bad", resource_index: 0, side: "sideways" },
      ],
      title_fields: { operation_count: 2 },
    });
    expect(preview?.files).toEqual([
      { path: "a.ts", rows: [{ kind: "delete", text: "old" }] },
      { path: "b.ts", rows: [{ kind: "insert", text: "new" }] },
    ]);
    expect(preview?.truncated).toBe(true);
    expect(preview && requestPreviewStat(preview)).toEqual({ added: 1, removed: 1 });
  });

  it("falls back to the text body when no structured lines arrived", () => {
    const edit = readRequestPreview({ affected_resources: ["a.ts"], body_preview: "- x\n+ y\n", kind: "file_edit" });
    expect(edit?.files[0]?.rows).toEqual([{ kind: "delete", text: "x" }, { kind: "insert", text: "y" }]);
    expect(edit && requestPreviewStat(edit)).toEqual({ added: 1, removed: 1 });
    expect(requestPreviewStat(readRequestPreview({ kind: "file_write", progress_fields: { new_line_count: 3 } })!)).toEqual({ added: 3, removed: null });
    const patch = readRequestPreview({
      affected_resources: ["b.ts"],
      body_preview: "*** Begin Patch\n*** Update File: b.ts\n@@\n ctx\n-x\n+y",
      kind: "apply_patch",
    });
    expect(patch?.files[0]?.rows.map((row) => row.kind)).toEqual(["context", "delete", "insert"]);
    expect(readRequestPreview({ kind: "file_write" })?.files).toEqual([]);
    expect(readRequestPreview(null)).toBeNull();
    expect(readRequestPreview({ title_fields: {} })).toBeNull();
  });

  it("builds a call that the finished call's card can show", () => {
    const edit = requestToolCall("file_edit", { bytes: 10, phase: "receiving", preview: readRequestPreview({ kind: "file_edit", title_fields: { path: "a.ts" } }) }, "s1", "running");
    expect(edit.input.path).toBe("a.ts");
    expect(resolveToolCard(edit).family).toBe("edit");
    const plan = requestToolCall("submit_plan", { bytes: 4, phase: "receiving", preview: readRequestPreview({ body_preview: "# Plan", kind: "submit_plan", title_fields: { name: "p" } }) }, "s2", "stopped");
    expect(plan.input.content).toBe("# Plan");
    expect(plan.state).toBe("stopped");
    expect(resolveToolCard(plan).family).toBe("plan");
    expect(resolveToolCard(requestToolCall("mcp_x_y", { bytes: 1, phase: "ready", preview: null }, "s3", "running")).family).toBe("other");
  });
});

