import type { ConversationEventHydrationV1Response, ConversationSessionConfigV1 } from "@/service/conversation-event-window-client";
import {
  clientActionId,
  type ConversationEventBody,
  type ConversationEventSummary,
  type PersistentChangeSet,
  type ToolPayload,
} from "@/service/conversation-event-window-store";

// Preview transcripts: fixed conversation windows shown by browser preview when no service runs. Each tool result
// follows the producer's envelope (model_output plus host_result, or a persisted change-set), so the cards render
// from the same facts as live sessions.
// 预览转录：浏览器预览在无服务时显示的固定会话窗口。每个工具结果遵循生产端信封（model_output 加 host_result，
// 或持久化 change-set），使卡片与实时会话依据相同事实渲染。

// PREVIEW_SERVICE_EPOCH identifies the fixed preview "service" generation.
// PREVIEW_SERVICE_EPOCH 标识固定的预览“服务”代次。
const PREVIEW_SERVICE_EPOCH = "browser-preview";
// PREVIEW_MODEL is the request every preview turn reports.
// PREVIEW_MODEL 是每个预览回合报告的请求。
const PREVIEW_MODEL = { provider: "deepseek", model: "deepseek-v4", thinking_level: "high" } as const;

// TranscriptBuilder appends events with consecutive seq numbers, monotonic timestamps and stable call ids.
// TranscriptBuilder 以连续 seq、单调时间戳与稳定调用标识追加事件。
class TranscriptBuilder {
  private readonly events: ConversationEventSummary[] = [];
  private turnId = 0;
  private apiCallId = 0;
  private toolCallId = 0;
  private inputId = 0;
  private clock: number;
  private turnStartSeq = 0;
  private turnStartedAt = 0;

  constructor(private readonly sessionId: string, startedMinutesAgo: number) {
    this.clock = Date.now() - startedMinutesAgo * 60_000;
  }

  // push appends one event and advances the clock by elapsedMs.
  // push 追加一条事件，并将时钟推进 elapsedMs。
  private push(event: ConversationEventBody, elapsedMs = 400): number {
    this.clock += elapsedMs;
    const seq = this.events.length + 1;
    this.events.push({
      seq,
      turn_id: this.turnId,
      created_at_ms: this.clock,
      updated_at_ms: this.clock,
      event_revision: 1,
      visibility: event.kind === "display_note" ? "ui_only" : "provider",
      event,
    });
    return seq;
  }

  // user opens a turn with one root prompt.
  // user 以一条根提示词开启一个回合。
  user(text: string): this {
    this.turnId += 1;
    this.inputId += 1;
    this.turnStartedAt = this.clock;
    this.turnStartSeq = this.push({
      kind: "user_input",
      client_action_id: clientActionId(`${this.sessionId}-input-${this.inputId}`),
      input_id: this.inputId,
      input_source: "root",
      text,
    }, 30_000);
    return this;
  }

  // api runs one provider call: optional reasoning, optional text, then its tool calls and stats.
  // api 运行一次 Provider 调用：可选思考、可选正文，然后是其工具调用与统计。
  api(step: { reasoning?: string; text?: string; tools?: PreviewToolCall[]; outputTokens?: number }): this {
    this.apiCallId += 1;
    const apiCallId = this.apiCallId;
    const preparedAt = this.clock;
    this.push({ kind: "api_started", api_call_id: apiCallId, purpose: "main_generation", request: { ...PREVIEW_MODEL } }, 200);
    const firstTokenAt = this.clock + 900;
    if (step.reasoning) this.push({ kind: "assistant_reasoning", api_call_id: apiCallId, text: step.reasoning, status: "completed" }, 2_400);
    if (step.text) this.push({ kind: "assistant_text", api_call_id: apiCallId, text: step.text, status: "completed" }, 1_800);
    for (const tool of step.tools ?? []) {
      this.toolCallId += 1;
      const startedAt = this.clock + 300;
      const finishedAt = startedAt + (tool.durationMs ?? 600);
      this.push({
        kind: "tool_call",
        api_call_id: apiCallId,
        tool_call_id: this.toolCallId,
        tool_name: tool.name,
        input: tool.input,
        ...(tool.output ? { output: tool.output } : {}),
        ...(tool.changeset ? { changeset: tool.changeset } : {}),
        status: tool.status ?? "completed",
        started_at_ms: startedAt,
        finished_at_ms: finishedAt,
        request_stream_id: `${this.sessionId}-tool-${this.toolCallId}`,
      }, (tool.durationMs ?? 600) + 300);
    }
    this.push({
      kind: "api_stats",
      api_call_id: apiCallId,
      purpose: "main_generation",
      ...PREVIEW_MODEL,
      usage: { input_tokens: 18_400 + apiCallId * 2_100, cache_read_tokens: 12_800, output_tokens: step.outputTokens ?? 420, reasoning_tokens: step.reasoning ? 260 : 0 },
      status: "completed",
      end_reason: "completed_successfully",
      continuation_eligibility: "forbidden",
      timing: { prepared_at_ms: preparedAt, dispatched_at_ms: preparedAt + 40, first_token_at_ms: firstTokenAt, completed_at_ms: this.clock },
    }, 100);
    return this;
  }

  // note adds one display-only notice inside the current turn.
  // note 在当前回合内添加一条仅显示的提示。
  note(noticeKind: Extract<ConversationEventBody, { kind: "display_note" }>["notice_kind"], severity: "info" | "warning" | "error", reasonCode: string, detail?: string): this {
    this.push({ kind: "display_note", notice_kind: noticeKind, severity, reason_code: reasonCode, ...(detail ? { redacted_detail: detail } : {}) });
    return this;
  }

  // end closes the turn with its boundary and aggregated statistics.
  // end 以回合边界与聚合统计关闭回合。
  end(): this {
    const boundarySeq = this.push({ kind: "turn_boundary", reason: "completed", outcome: "completed", purpose: "business" }, 50);
    const toolCalls = this.events.filter((summary) => summary.turn_id === this.turnId && summary.event.kind === "tool_call").length;
    const apiCalls = this.events.filter((summary) => summary.turn_id === this.turnId && summary.event.kind === "api_started").length;
    const bucket = {
      api_call_count: apiCalls,
      unknown_usage_api_call_count: 0,
      tool_call_count: toolCalls,
      subagent_return_count: 0,
      usage: { input_tokens: 21_000 * apiCalls, cache_read_tokens: 12_800 * apiCalls, output_tokens: 520 * apiCalls, reasoning_tokens: 260 * apiCalls },
    };
    this.push({
      kind: "turn_stats",
      start_seq: this.turnStartSeq,
      end_seq: boundarySeq,
      outcome: "completed",
      duration_ms: this.clock - this.turnStartedAt,
      buckets: { direct: bucket, subagent: { ...bucket, api_call_count: 0, tool_call_count: 0, usage: {} }, total: bucket },
    }, 10);
    return this;
  }

  // window wraps the events into one hydration response.
  // window 将事件封装为一份 hydration 响应。
  window(config: Partial<ConversationSessionConfigV1> = {}): ConversationEventHydrationV1Response {
    return previewWindow(this.sessionId, this.events, config);
  }
}

// PreviewToolCall is one tool call inside a preview provider step.
// PreviewToolCall 是预览 Provider 步骤中的一次工具调用。
interface PreviewToolCall {
  name: string;
  input: ToolPayload;
  output?: ToolPayload;
  changeset?: PersistentChangeSet;
  status?: Extract<ConversationEventBody, { kind: "tool_call" }>["status"];
  durationMs?: number;
}

// toolOutput builds the durable success envelope the runtime bridge persists for one native tool.
// toolOutput 构建运行时桥为一个原生工具持久化的可靠成功信封。
function toolOutput(name: string, modelOutput: string, hostResult: { kind: string; payload: ToolPayload } | null = null, isError = false): ToolPayload {
  return { tool_call_id: name, tool_name: name, model_output: modelOutput, host_result: hostResult, is_error: isError };
}

// previewWindow builds one complete hydration response around fixed events.
// previewWindow 围绕固定事件构建一份完整 hydration 响应。
function previewWindow(
  sessionId: string,
  events: ConversationEventSummary[],
  config: Partial<ConversationSessionConfigV1> = {},
): ConversationEventHydrationV1Response {
  return {
    session_id: sessionId,
    window_generation: 1,
    service_epoch: PREVIEW_SERVICE_EPOCH,
    memory_revision: 1,
    durable_revision: 1,
    events,
    older_before_seq: null,
    has_more: false,
    open_turn_id: null,
    open_turn_start_seq: null,
    session_config: {
      answer_mode: { mode: "standard", revision: 0, prepared: null },
      permission_mode: "workspace_write",
      current_request: { provider: PREVIEW_MODEL.provider, connection_id: null, model: PREVIEW_MODEL.model, thinking_level: PREVIEW_MODEL.thinking_level, temperature: null, set_at_ms: null },
      next_request: null,
      context_compaction_window_node_id: null,
      dedicated: null,
      ...config,
    },
  };
}

// fileReadOutput renders a file_read result exactly as the file runtime does: numbered lines inside a fence.
// fileReadOutput 按文件运行时的方式渲染 file_read 结果：围栏内带行号的行。
function fileReadOutput(path: string, language: string, lines: string[], startLine = 1, totalLines = lines.length): ToolPayload {
  const end = startLine + lines.length - 1;
  let markdown = `# File: ${path}\n\nLines: ${totalLines} | Showing: ${startLine}-${end} | Encoding: utf-8 (detected)\n\n\`\`\`${language}\n`;
  const outputStartLine = markdown.split("\n").length;
  markdown += lines.map((line, index) => `L${startLine + index}: ${line}\n`).join("") + "```\n";
  return toolOutput("file_read", markdown, {
    kind: "file_access",
    payload: {
      schema: "vulcan.file_read.v3",
      action: "read",
      path,
      resource_id: "resource-1",
      language,
      encoding: "utf-8",
      encoding_source: "detected",
      encoding_certain: true,
      bom: false,
      line_ending: "LF",
      mixed_line_endings: false,
      final_newline: true,
      total_line_count: totalLines,
      read_status: "content",
      requested_range: { start_line: startLine, end_line: end },
      range: { source_start_line: startLine, line_count: lines.length },
      render: { output_start_line: outputStartLine },
      truncated: false,
      next_start_line: null,
    },
  });
}

// shellOutput renders a shell_command result: the text body plus the command_run host payload.
// shellOutput 渲染 shell_command 结果：正文加 command_run 宿主载荷。
function shellOutput(command: string, exitCode: number, stdout: string, stderr = "", durationMs = 1_800): ToolPayload {
  const body = [`Command: ${command}`, `Exit code: ${exitCode}`, ...(stdout ? [`\n[stdout]\n${stdout}`] : []), ...(stderr ? [`\n[stderr]\n${stderr}`] : [])].join("\n");
  return toolOutput("shell_command", body, {
    kind: "command_run",
    payload: {
      session_id: null,
      command,
      cwd: "D:/openvulcan/vulcan-code/apps/vulcan-code-web",
      shell: "pwsh",
      status: exitCode === 0 ? "completed" : "failed",
      exit_code: exitCode,
      duration_ms: durationMs,
      stdout_bytes: stdout.length,
      stderr_bytes: stderr.length,
      combined_output_overflowed: false,
      failure_reason: null,
    },
  }, exitCode !== 0);
}

// editOutput summarizes an applied change-set the way the file runtime reports it to the model.
// editOutput 以文件运行时向模型报告的方式汇总已应用的 change-set。
function editOutput(changeset: PersistentChangeSet): ToolPayload {
  const lines = changeset.files.map((file) => {
    const added = file.hunks.reduce((total, hunk) => total + hunk.inserted.length, 0);
    const removed = file.hunks.reduce((total, hunk) => total + hunk.deleted.length, 0);
    return `- ${file.change}: \`${file.path ?? file.new_path ?? ""}\` · +${added} / -${removed} lines`;
  });
  return toolOutput("file_edit", `# File changes applied\n\n${lines.join("\n")}`);
}

// READING_WIDTH_CHANGESET is the edit shown by the migration preview session.
// READING_WIDTH_CHANGESET 是迁移预览会话展示的编辑。
const READING_WIDTH_CHANGESET: PersistentChangeSet = {
  mode: "applied",
  files: [{
    change: "modify",
    path: "src/ui-kit/styles/tokens.css",
    hunks: [
      {
        before: "  --vk-read-w: 46rem;\n  /* The conversation column: the reading measure plus its side padding, or the whole stage at \"full\". */",
        after: "",
        deleted: [{ line: 51, content: "  --vk-column-w: calc(var(--vk-read-w) + var(--vk-column-pad) * 2);" }],
        inserted: [
          { line: 51, content: "  --vk-read-col: calc(var(--vk-read-w) + var(--vk-column-pad) * 2);" },
          { line: 52, content: "  --vk-column-w: var(--vk-read-col);" },
          { line: 53, content: "  --vk-reserve-w: calc(var(--vk-read-col) + var(--vk-u) * 10);" },
        ],
      },
    ],
    before_text_format: { encoding: "utf-8", line_ending: "LF", final_newline: true },
    after_text_format: { encoding: "utf-8", line_ending: "LF", final_newline: true },
  }],
};

// migrationPreview is a complete coding turn: read, edit, verify, answer.
// migrationPreview 是一个完整编码回合：读取、编辑、验证、回答。
function migrationPreview(sessionId: string): ConversationEventHydrationV1Response {
  return new TranscriptBuilder(sessionId, 12)
    .user("The context panel docks too late on the wide reading width. Make the reserved width follow the reading-width preset.")
    .api({
      reasoning: "The reserve is a fixed 922px constant in the layout code, while the reading column is defined in rem tokens. I should derive the reserve from the same tokens so both move together, then read it back as a registered length.",
      text: "Let me look at how the column width is defined first.",
      tools: [{
        name: "file_read",
        input: { path: "src/ui-kit/styles/tokens.css", start_line: 44, end_line: 56 },
        output: fileReadOutput("src/ui-kit/styles/tokens.css", "css", [
          "  /* Shell geometry, in units so it grows with the tier. */",
          "  --vk-rail-w: calc(var(--vk-u) * 12);",
          "  --vk-sidebar-w: calc(var(--vk-u) * 66);",
          "  --vk-panel-w: calc(var(--vk-u) * 94);",
          "  --vk-read-w: 46rem;",
          "  /* The conversation column: the reading measure plus its side padding, or the whole stage at \"full\". */",
          "  --vk-column-pad: calc(var(--vk-u) * 8);",
          "  --vk-column-w: calc(var(--vk-read-w) + var(--vk-column-pad) * 2);",
        ], 44, 214),
      }],
    })
    .api({
      text: "The column is `--vk-read-w` plus padding. I'll name that sum and derive the reserve from it.",
      tools: [{ name: "file_edit", input: { path: "src/ui-kit/styles/tokens.css", operations: 1 }, output: editOutput(READING_WIDTH_CHANGESET), changeset: READING_WIDTH_CHANGESET }],
    })
    .api({
      tools: [{
        name: "shell_command",
        input: { command: "pnpm vitest run src/components/workspace" },
        output: shellOutput("pnpm vitest run src/components/workspace", 0, " ✓ src/components/workspace/workspace-shell/workspace-panel-layout.test.ts (4 tests) 38ms\n ✓ src/components/workspace/workspace-control-preaudit.test.tsx (12 tests) 412ms\n\n Test Files  24 passed (24)\n      Tests  86 passed (86)\n   Duration  4.21s", "", 4_210),
        durationMs: 4_210,
      }],
    })
    .api({
      text: [
        "The reserve now follows the reading width.",
        "",
        "| Preset | Column | Reserve |",
        "| --- | --- | --- |",
        "| Compact | 38rem + padding | 598px |",
        "| Standard | 46rem + padding | 702px |",
        "| Wide | 58rem + padding | 858px |",
        "",
        "The layout reads it back through a registered length, the same way it reads the navigation widths:",
        "",
        "```ts",
        "export function conversationReservedWidth(): number {",
        "  return readLengthToken(\"--vk-reserve-w\");",
        "}",
        "```",
        "",
        "All 86 workspace tests pass.",
      ].join("\n"),
      outputTokens: 640,
    })
    .end()
    .window();
}

// introduction is a short text-only exchange.
// introduction 是一段简短的纯文本对话。
function introduction(sessionId: string): ConversationEventHydrationV1Response {
  return new TranscriptBuilder(sessionId, 30)
    .user("Introduce yourself.")
    .api({
      reasoning: "A short self-introduction: what I can do in this workspace and how to steer me.",
      text: "I'm the coding agent for this workspace. I can read and edit files, run commands in the project terminal, search the knowledge base and hand longer tasks to subagents.\n\nTell me what you want changed and I'll show my work as I go — every edit appears as a diff you can review before you commit.",
    })
    .end()
    // Follow-up turns give the prompt navigator enough entries to appear.
    // 后续回合让提示词导航有足够条目而出现。
    .user("What can't you do?")
    .api({ text: "I can't reach services outside the permissions you grant, and I won't push or publish without asking first." })
    .end()
    .user("How do I hand you a long task?")
    .api({ text: "Describe the goal and the finish line. I'll plan it, split the parts that can run in parallel across subagents and report back when each part lands." })
    .end()
    .window();
}

// GALLERY_TODOS is the checklist the tool-card gallery reports.
// GALLERY_TODOS 是工具卡展示会话报告的清单。
const GALLERY_TODOS = [
  { step: "Rebuild file and command cards", status: "completed" },
  { step: "Rebuild document and planning cards", status: "in_progress" },
  { step: "Rebuild subagent and provider cards", status: "pending" },
];

// toolCardGallery exercises every rebuilt tool card, including failed, stopped and running calls.
// toolCardGallery 覆盖每张重建的工具卡，包括失败、停止与运行中的调用。
function toolCardGallery(sessionId: string): ConversationEventHydrationV1Response {
  return new TranscriptBuilder(sessionId, 60)
    .user("Audit the tool card layouts against the real tool results.")
    .api({
      text: "I'll track the audit and load the terminal skill first.",
      tools: [
        { name: "todolist", input: { items: GALLERY_TODOS }, output: toolOutput("todolist", "Todolist updated", { kind: "todolist", payload: { explanation: "Cards move family by family.", items: GALLERY_TODOS } }) },
        { name: "skill", input: { name: "terminal-session" }, output: toolOutput("skill", "<skill_content name=\"terminal-session\">\n# Terminal Session\n\nKeep long-running processes in a **persistent terminal** and poll it with `read`.\n</skill_content>") },
        { name: "get_goal", input: {}, output: toolOutput("get_goal", "# Goal\n\nStatus: active\n\n## Objective\n\nReplace every legacy tool card with a purpose-built card\n\nTokens used: 182400\n\nToken budget: 400000\n\nRemaining tokens: 217600\n\nElapsed seconds: 2710\n") },
      ],
    })
    .api({
      tools: [
        { name: "knowledge_search", input: { queries: ["tool card layout", "diffstat"] }, output: toolOutput("knowledge_search", "## Hits\n\n1. **Tool card anatomy** · `design/tool-cards.md` · score 0.82\n   > One head row: family tile, verb, target, facts, state.\n2. **Diff colors** · `design/tokens.md` · score 0.64\n   > Additions use the ok tone; removals use the bad tone.") },
        {
          name: "spawn_agent",
          input: { task_name: "provider-cards", message: "Rebuild the provider **search** and **extract** cards.\n\n- Read `provider-tool-presentation.ts` for the result shapes\n- Keep sources clickable" },
          output: toolOutput("spawn_agent", "# Agent scheduled\n\nName: `provider-cards`\nSession ID: `child-1`\nExecution: `running`\n\nScheduling is not completion. Use `wait_agents_result` for subsequent results.", {
            kind: "subagent_receipt_v1",
            payload: { format: "markdown", task_id: "provider-cards", subagent_task_id: "child-1", display_title: "provider-cards", execution_id: "execution-1", execution_status: "running" },
          }),
        },
        { name: "web_search", input: { query: "accessible disclosure pattern" }, output: toolOutput("web_search", "# Search: `accessible disclosure pattern`\n\n1. [Disclosure (Show/Hide) Pattern \\| APG](<https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/>)\n\nA disclosure is a widget that enables content to be either collapsed (hidden) or expanded (visible). It has two elements: a disclosure button and a section of content.\n\n2. [details: The Details disclosure element](<https://developer.mozilla.org/en-US/docs/Web/HTML/Element/details>)\n\nThe details HTML element creates a disclosure widget in which information is visible only when the widget is toggled into an open state.\n") },
        {
          name: "vulcan_web_search",
          input: { query: "site:developers.openai.com/codex authentication -\"api key\" lang:en", max_results: 10 },
          output: toolOutput("vulcan_web_search", `# Search: \`site:developers.openai.com/codex authentication -"api key" lang:en\`\n${
            ["Authentication", "Sign in with ChatGPT", "CLI configuration", "Security", "Sandboxing", "Approvals", "Enterprise admin"]
              .map((title, index) => `\n${index + 1}. [${title} \\| Codex](<https://developers.openai.com/codex/${title.toLowerCase().replace(/ /g, "-")}>)\n\nHow Codex handles ${title.toLowerCase()} for local and cloud tasks.\n`).join("")}`),
        },
        { name: "web_extract", input: { urls: ["https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/", "https://example.test/missing"] }, output: toolOutput("web_extract", "# Extracted pages\n\n## [https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/](<https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/>)\n\n## About This Pattern\n\nA disclosure is a widget that enables content to be either collapsed or expanded.\n\n## Keyboard Interaction\n\n- **Enter**: activates the disclosure control.\n- **Space**: activates the disclosure control.\n\n## Failed sources\n\n- [https://example.test/missing](<https://example.test/missing>): HTTP 404 Not Found\n") },
        {
          name: "runlua",
          input: { task: "count tool families", code: "local families = { \"read\", \"search\", \"edit\", \"run\" }\nprint(#families .. \" families\")\nreturn families" },
          output: toolOutput("runlua", "4 families\n\n```json\n[\"read\", \"search\", \"edit\", \"run\"]\n```", { kind: "lua_execution", payload: { tool: "runlua", mode: "one_shot", action: "eval", status: "completed", task: "count tool families", elapsedMs: 38 } }),
        },
        {
          name: "ask",
          input: { questions: [
            { header: "Density", question: "How dense should collapsed tool cards be?", options: [{ label: "One line", description: "" }, { label: "Two lines", description: "" }] },
            { header: "Notes", question: "Anything else to keep from the old cards?" },
          ] },
          output: toolOutput("ask", "## User Answers\n\n- Density: One line\n- Notes: Unanswered\n\nContinue with these answers.", { kind: "ask", payload: { format: "markdown" } }),
        },
        {
          name: "knowledge_write",
          input: { libraryId: "lib-design", action: "edit", path: "design/tool-cards.md", edits: [{ oldText: "Cards keep the legacy header.", newText: "Cards use one head row:\nfamily tile, verb, target, facts, state." }] },
          output: toolOutput("knowledge_write", "# Knowledge change\n\nLibrary ID: `lib-design`\nChange seq: 14\nPending embeddings: 1\n", { kind: "knowledge_write_v1", payload: { library_id: "lib-design", change_seq: 14, documents: [{ id: "doc-3", path: "design/tool-cards.md", revision: 7 }], pending_embeddings: 1 } }),
        },
        { name: "read_media", input: { path: "docs/design/tool-card-sketch.png" }, output: toolOutput("read_media", JSON.stringify({ $vulcan_native_media_input: 1, text: "Attached image tool-card-sketch.png.", resource: { file_name: "tool-card-sketch.png" } })) },
        { name: "mcp_github_list_pull_requests", input: { owner: "openvulcan", repo: "vulcan-code", state: "open" }, output: toolOutput("mcp_github_list_pull_requests", "| # | Title | Author |\n| --- | --- | --- |\n| 412 | Rebuild tool cards | lingeasy |\n| 409 | Split the kit tokens | lingeasy |"), durationMs: 840 },
        { name: "file", input: { action: "list", path: "src/conversation/tools" }, output: toolOutput("file", "- `cards` · directory\n- `views` · directory\n- `tool-call.ts` · file · 4210 bytes\n- `tool-card.tsx` · file · 5032 bytes\n- `tool-call-card.tsx` · file · 3388 bytes") },
        {
          name: "terminal_session",
          input: { action: "read", terminal_session_id: "term-7" },
          output: toolOutput("terminal_session", "Session term-7 is running.", {
            kind: "terminal_session",
            payload: {
              action: "read",
              terminal_session_id: "term-7",
              name: "vite",
              status: "running",
              events: [
                { seq: 1, kind: "output", stream: "stdout", text: "\u001b[32m  VITE v7.1.4\u001b[39m  ready in \u001b[1m612 ms\u001b[22m\n\n  ➜  Local:   http://127.0.0.1:17611/\n" },
                { seq: 2, kind: "output", stream: "stdout", text: "\u001b[2m10:42:07\u001b[22m \u001b[36m[vite]\u001b[39m hmr update /src/conversation/tools/tool-card.tsx\n" },
              ],
            },
          }),
        },
      ],
    })
    .api({
      text: "The type check fails on one card; the full build was cancelled.",
      tools: [
        {
          name: "shell_command",
          input: { command: "pnpm run typecheck" },
          output: shellOutput("pnpm run typecheck", 2, "", "src/conversation/tools/cards/plan-cards.tsx(58,7): \u001b[31merror\u001b[39m TS2322: Type 'string | null' is not assignable to type 'string'.", 6_400),
          durationMs: 6_400,
        },
        { name: "shell_command", input: { command: "pnpm run build" }, status: "interrupted", durationMs: 2_000 },
        {
          name: "submit_plan",
          input: { name: "tool-card-migration" },
          output: toolOutput("submit_plan", "# Plan saved\n\nName: `tool-card-migration`\n\nPath: `docs/plan/tool-card-migration.md`\n", {
            kind: "plan_artifact",
            payload: {
              name: "tool-card-migration",
              path: "docs/plan/tool-card-migration.md",
              content: "## Steps\n\n1. Fix the nullable target in the todo card.\n2. Rebuild the subagent cards with delivery progress.\n3. Rebuild provider search and extract cards.\n\n## Verification\n\n- `pnpm run build`\n- Preview gallery session",
            },
          }),
        },
      ],
    })
    .end()
    .window();
}

// fence wraps one answer component body in its fence, so the gallery text needs no escaped backticks.
// fence 用围栏包裹一个回答组件正文，使展示文本无需转义反引号。
function fence(info: string, ...lines: string[]): string {
  return ["```" + info, ...lines, "```"].join("\n");
}

// ANSWER_GALLERY is a final answer that uses every answer component, in the order a review would.
// ANSWER_GALLERY 是一条使用全部回答组件的最终回答，按评审回答的顺序排列。
const ANSWER_GALLERY = [
  "The rebuilt answer components are ready to ship; two small risks remain in the diagram layout.",
  fence("ui-metrics", "Components | 24", "Tests passed | 41 | 41 | tests | ok", "Bundle | 612 | 560 | KB", "Coverage | 81 | 100 | % | warn"),
  "### What changed",
  fence("ui-files", "src/ui-kit/styles/answer.css | +312 -0 | Kit answer stylesheet", "src/styles/answer-components.css | +0 -1287 | Legacy stylesheet", "src/components/conversation/answer-components/answer-diagrams.tsx | +6 -6"),
  "### Review",
  fence("ui-findings", "high | Sequence labels show the panel colour over a tinted canvas | src/ui-kit/styles/answer.css:212", "medium | ER key badges overlap long field names | src/components/conversation/answer-components/answer-diagrams.tsx:334", "low | Chart legend wraps before the plot on narrow widths", "info | Gallery thumbnails crop portrait images"),
  fence("ui-compare", "Criterion | Kit badge | Custom pill", "Matches the kit | ok | no Needs its own tones", "Severity tones | ok Mapped through data-tone | ok", "Extra CSS | ok None | warn 20 lines"),
  fence("ui-checklist", "ok | Square marks everywhere | Status, steps and decisions", "ok | Tones use kit tokens", "warn | Dark theme contrast on PK badges | Check in the command skin", "todo | Snapshot the gallery in all three themes"),
  fence("ui-steps", "done | Translate the stylesheet | One kit file, loaded with the renderer", "now | Check every component in the preview", "todo | Remove the legacy pill styles"),
  fence("ui-timeline", "09:10 | Stylesheet translated", "09:40 | Tests updated | Selectors moved to vk-a-*", "10:05 | Preview gallery added"),
  fence("ui-cards", "Keep fences lazy | The parser and styles load on the first closed fence. | ok Recommended", "Inline everything | Startup grows by about 40 KB. | no Rejected"),
  fence("ui-kv", "Stylesheet | src/ui-kit/styles/answer.css", "Loaded | With the component renderer", "Tokens | Kit colours and spacing"),
  fence("ui-chart bar Bundle size by chunk", "Chunk | Before | After", "Workbench | 420 | 396", "Answers | 88 | 64", "Diagrams | 104 | 98"),
  fence("ui-chart donut Rules by area", "Area | Rules", "Lists | 18", "Diagrams | 46", "Decisions | 14", "Charts | 16"),
  "### How a fence renders",
  fence("ui-flow LR", "(Reply) -> {Fence closed?} -> Parse -> [(Reply store)]", "{Fence closed?} --> Show code : no", "+ Parse -> Lazy styles"),
  fence("ui-sequence", "Reply -> Markdown: closed fence", "Markdown -> Renderer: parse", "note Renderer: lazy chunk", "== Paint ==", "Renderer --> Markdown: component", "~ Markdown -> Markdown: measure"),
  fence("ui-er", "Session", "  id PK", "  title | shown in the sidebar", "Reply", "  id PK", "  session_id -> Session", "  + answer_mode"),
  fence("ui-tree", "src", "  ui-kit", "    styles", "      + answer.css | lazy", "      prose.css", "  - styles/answer-components.css"),
  fence("ui-math Meter fill", "w = \\min\\left(\\frac{v}{m}, 1\\right)", "", "w | fill width", "v | value", "m | maximum"),
  "The meter fills to $`w = v / m`$ and turns red past the maximum.",
  fence("ui-callout warning Check the dark themes", "PK and FK badges mix their tone with the panel; confirm the contrast in the command skin."),
  "````ui-details Why the marks are square",
  "The kit draws checkboxes and radios as squares, so status marks, step numbers and decision marks follow.",
  "",
  fence("ui-kv", "Checkbox | square with a check", "Radio | square with an inner square"),
  "````",
  "````ui-tabs",
  "### Light",
  "Panels are white on a grey ground.",
  "### Dark",
  "Panels lift one step above the ground.",
  "````",
  "````ui-columns",
  "### Keep",
  "- Lazy loading",
  "- Kit tones",
  "### Drop",
  "- Rounded pills",
  "- Legacy tokens",
  "````",
  fence("ui-code", "src/ui-kit/styles/answer.css:1-12 | 3-4 | Header comment"),
  fence("ui-decision", "Ship the gallery fixture with the release?", "* Keep it | Useful for every later visual check", "- Remove it | Smaller preview data"),
].join("\n\n");

// answerGallery renders every answer component in one final reply.
// answerGallery 在一条最终回答中渲染全部回答组件。
function answerGallery(sessionId: string): ConversationEventHydrationV1Response {
  return new TranscriptBuilder(sessionId, 20)
    .user("Review the rebuilt answer components and tell me if they are ready.")
    .api({ text: ANSWER_GALLERY })
    .end()
    .window();
}

// FIXTURES maps preview session ids to their transcript builders.
// FIXTURES 将预览会话标识映射到其转录构建函数。
const FIXTURES: Record<string, (sessionId: string) => ConversationEventHydrationV1Response> = {
  preview: migrationPreview,
  "preview-intro": introduction,
  "preview-cards": toolCardGallery,
  "preview-answers": answerGallery,
};

// PREVIEW_TRANSCRIPT_SESSION_IDS lists the preview sessions that carry a transcript.
// PREVIEW_TRANSCRIPT_SESSION_IDS 列出带有转录的预览会话。
export const PREVIEW_TRANSCRIPT_SESSION_IDS: readonly string[] = Object.keys(FIXTURES);

// previewConversationWindow returns the fixture window for one preview session, or an empty window.
// previewConversationWindow 返回一个预览会话的样本窗口；没有样本时返回空窗口。
export function previewConversationWindow(sessionId: string): ConversationEventHydrationV1Response {
  return FIXTURES[sessionId]?.(sessionId) ?? previewWindow(sessionId, []);
}
